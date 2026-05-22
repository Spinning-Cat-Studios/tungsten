//! TyVar validation and post-elaboration substitution.

use tungsten_bootstrap::driver::AdtTypes;
use tungsten_bootstrap::elaborate::TypeProvenance;

/// Validate TyVar escapes after elaboration (W3.1 Tool 2).
///
/// Reports definitions with free TyVars in their Core IR term bodies.
/// Forall-typed (polymorphic) defs are skipped — they're monomorphized during codegen.
///
/// # Known Violations (ADR 18.4.26i)
///
/// **Bug A (~299 escapes):** Mutually recursive ADTs (TypeExpr ↔ Expr, etc.) produce
/// spurious TyVar escapes because the current μ-type encoding doesn't support mutual
/// recursion — cross-references remain as bare TyVars. Pending Phase 2 fix (nested
/// μ-binder group encoding).
///
/// **Bug B (~4 escapes):** Fixed. Fold/unfold annotations in monomorphic functions
/// retained generic TyVar("T") due to provenance key collisions. Now resolved by
/// structural extraction from μ-binder bodies in `apply_tyvar_substitutions`.
///
/// The Bug A escape count MUST NOT increase. If it does, it indicates a regression
/// in the type encoding or a new source of TyVar leaks.
pub(super) fn validate_tyvar_escapes(
    defs: &[tungsten_bootstrap::elaborate::CoreDef],
    check_tyvar_escape: bool,
) {
    let should_check = check_tyvar_escape || cfg!(debug_assertions);
    if !should_check {
        return;
    }

    let mut escape_count = 0;
    let mut def_count = 0;
    for def in defs {
        if matches!(&def.ty, tungsten_core::types::Type::Forall(_, _)) {
            continue;
        }
        let free = def.term.free_type_vars();
        let genuine_leaks: std::collections::HashSet<_> =
            free.into_iter().filter(|v| !v.starts_with('@')).collect();
        if !genuine_leaks.is_empty() {
            def_count += 1;
            escape_count += genuine_leaks.len();
            if check_tyvar_escape {
                eprintln!(
                    "[tyvar-escape] {}: body contains free TyVar(s): {:?}",
                    def.name, genuine_leaks
                );
            }
        }
    }
    if def_count > 0 && check_tyvar_escape {
        eprintln!(
            "\n[tyvar-escape] Summary: {} definition(s) with TyVar escapes ({} total occurrence(s))",
            def_count, escape_count
        );
    }
}

/// Apply transitional TyVar substitutions to repair malformed Core IR (W3.2).
pub(super) fn apply_tyvar_substitutions(
    defs: &mut [tungsten_bootstrap::elaborate::CoreDef],
    type_provenance: &TypeProvenance,
    adt_types: &AdtTypes,
    verbose: bool,
) {
    let mut substitution_count = 0;
    for def in defs.iter_mut() {
        if matches!(&def.ty, tungsten_core::types::Type::Forall(_, _)) {
            continue;
        }
        let free = def.term.free_type_vars();
        let genuine_leaks: std::collections::HashSet<_> =
            free.into_iter().filter(|v| !v.starts_with('@')).collect();
        if genuine_leaks.is_empty() {
            continue;
        }

        let subst = extract_type_param_substitution(&def.ty, type_provenance, adt_types);
        if subst.is_empty() {
            continue;
        }

        if subst.keys().any(|k| genuine_leaks.contains(k)) {
            if verbose {
                eprintln!(
                    "[tyvar-cleanup] {}: substituting {:?}",
                    def.name,
                    subst
                        .iter()
                        .filter(|(k, _)| genuine_leaks.contains(*k))
                        .collect::<Vec<_>>()
                );
            }
            def.term.term = def.term.term.substitute_type_vars(&subst);
            substitution_count += 1;

            let remaining = def.term.free_type_vars();
            let remaining_leaks: std::collections::HashSet<_> = remaining
                .into_iter()
                .filter(|v| !v.starts_with('@'))
                .collect();
            if !remaining_leaks.is_empty() {
                debug_assert!(
                    false,
                    "Post-elab TyVar substitution incomplete for '{}': \
                     remaining free TyVars: {:?}",
                    def.name, remaining_leaks
                );
                eprintln!(
                    "[tyvar-cleanup warning] '{}': substitution incomplete, \
                     {} free TyVar(s) remain: {:?}",
                    def.name,
                    remaining_leaks.len(),
                    remaining_leaks
                );
            }
        }
    }
    if substitution_count > 0 && verbose {
        eprintln!(
            "[tyvar-cleanup] Substituted TyVars in {} definition(s)",
            substitution_count
        );
    }
}

/// Extract type parameter substitutions from a definition's type using TypeProvenance.
///
/// When provenance gives no-op substitutions (e.g., T → TyVar("T") due to key
/// collisions from pattern/unification encodings), falls back to structural
/// extraction: compares each μ-binder body in the def type against a template
/// built from the ADT's raw constructor structure to extract concrete mappings.
fn extract_type_param_substitution(
    def_type: &tungsten_core::types::Type,
    provenance: &TypeProvenance,
    adt_types: &AdtTypes,
) -> std::collections::HashMap<String, tungsten_core::types::Type> {
    use std::collections::HashMap;
    use tungsten_core::types::Type;

    let mut subst: HashMap<String, Type> = HashMap::new();

    fn find_mu_binders(ty: &Type, binders: &mut Vec<String>) {
        match ty {
            Type::Mu(binder, body) => {
                binders.push(binder.clone());
                find_mu_binders(body, binders);
            }
            Type::Arrow(t1, t2) | Type::Product(t1, t2) | Type::Sum(t1, t2) => {
                find_mu_binders(t1, binders);
                find_mu_binders(t2, binders);
            }
            Type::Forall(_, body) => find_mu_binders(body, binders),
            Type::Ptr(inner) | Type::Ref(inner) => find_mu_binders(inner, binders),
            Type::App(_, args) | Type::Adt(_, args, _) => {
                for arg in args {
                    find_mu_binders(arg, binders);
                }
            }
            Type::Eq(ty_eq, _, _) => find_mu_binders(ty_eq, binders),
            _ => {}
        }
    }

    let mut binders = Vec::new();
    find_mu_binders(def_type, &mut binders);

    // Step 1: Try provenance-based extraction (original logic).
    for binder in &binders {
        if let Some(origin) = provenance.mu_origins.get(binder) {
            if let Some((params, _)) = adt_types.get(&origin.adt_name) {
                for (param, arg) in params.iter().zip(&origin.type_args) {
                    subst.insert(param.clone(), arg.clone());
                }
            }
        }
    }

    // Step 2: Structural extraction from μ-binder bodies in the def type.
    // This overrides provenance results when it finds better (more concrete)
    // mappings. Provenance can give wrong results due to key collisions:
    // all instantiations of List<X> share the same α_List key, so the last
    // encoding wins, which may be from a different instantiation or from a
    // pattern/unification call with TyVar placeholders.
    let mut structural_subst: HashMap<String, Type> = HashMap::new();
    extract_subst_from_mu_bodies(def_type, adt_types, &mut structural_subst);

    // Merge: structural results override provenance results.
    for (param, arg) in structural_subst {
        subst.insert(param, arg);
    }

    // Remove any remaining no-op entries that couldn't be resolved.
    subst.retain(|k, v| !matches!(v, Type::TyVar(n) if n == k));

    subst
}

/// Walk a type tree, finding μ-binders and structurally extracting type param
/// substitutions from their bodies by comparing with ADT constructor templates.
fn extract_subst_from_mu_bodies(
    ty: &tungsten_core::types::Type,
    adt_types: &AdtTypes,
    subst: &mut std::collections::HashMap<String, tungsten_core::types::Type>,
) {
    use tungsten_core::types::Type;

    match ty {
        Type::Mu(binder, body) => {
            if let Some(name) = binder.strip_prefix("α_") {
                if let Some((params, constructors)) = adt_types.get(name) {
                    let template = build_adt_body_template(name, binder, constructors);
                    extract_mappings_structural(&template, body, params, subst);
                }
            }
            extract_subst_from_mu_bodies(body, adt_types, subst);
        }
        Type::Arrow(t1, t2) | Type::Product(t1, t2) | Type::Sum(t1, t2) => {
            extract_subst_from_mu_bodies(t1, adt_types, subst);
            extract_subst_from_mu_bodies(t2, adt_types, subst);
        }
        Type::Forall(_, body) | Type::Ptr(body) | Type::Ref(body) => {
            extract_subst_from_mu_bodies(body, adt_types, subst);
        }
        Type::App(_, args) | Type::Adt(_, args, _) => {
            for arg in args {
                extract_subst_from_mu_bodies(arg, adt_types, subst);
            }
        }
        _ => {}
    }
}

/// Build a template body from raw ADT constructor fields.
/// Self-references to the ADT are replaced with TyVar(mu_var).
fn build_adt_body_template(
    adt_name: &str,
    mu_var: &str,
    constructors: &[tungsten_bootstrap::elaborate::Constructor],
) -> tungsten_core::types::Type {
    use tungsten_core::types::Type;

    let ctor_types: Vec<Type> = constructors
        .iter()
        .map(|ctor| build_ctor_payload_template(ctor, adt_name, mu_var))
        .collect();

    if ctor_types.is_empty() {
        Type::Void
    } else if ctor_types.len() == 1 {
        ctor_types.into_iter().next().unwrap()
    } else if ctor_types.len() == 2 {
        let mut iter = ctor_types.into_iter();
        Type::sum(iter.next().unwrap(), iter.next().unwrap())
    } else {
        // 3+ constructors use Adt representation — build matching Adt template
        let variants: Vec<(String, Type)> = constructors
            .iter()
            .zip(ctor_types.into_iter())
            .map(|(ctor, ty)| (ctor.name.clone(), ty))
            .collect();
        Type::adt(adt_name.to_string(), Vec::new(), variants)
    }
}

/// Build a constructor payload template, replacing self-references with the μ-var.
fn build_ctor_payload_template(
    ctor: &tungsten_bootstrap::elaborate::Constructor,
    adt_name: &str,
    mu_var: &str,
) -> tungsten_core::types::Type {
    use tungsten_core::types::Type;

    if ctor.fields.is_empty() {
        Type::Unit
    } else if ctor.fields.len() == 1 {
        replace_self_refs_in_field(&ctor.fields[0], adt_name, mu_var)
    } else {
        // Left-nested product (matches encode_constructor_type_impl)
        let mut product = replace_self_refs_in_field(&ctor.fields[0], adt_name, mu_var);
        for field in &ctor.fields[1..] {
            let field_ty = replace_self_refs_in_field(field, adt_name, mu_var);
            product = Type::product(product, field_ty);
        }
        product
    }
}

/// Replace top-level self-references to the ADT with the μ-variable.
fn replace_self_refs_in_field(
    ty: &tungsten_core::types::Type,
    adt_name: &str,
    mu_var: &str,
) -> tungsten_core::types::Type {
    use tungsten_core::types::Type;
    match ty {
        Type::TyVar(name) => {
            let stripped = name.strip_prefix('@').unwrap_or(name);
            if stripped == adt_name {
                Type::TyVar(mu_var.to_string())
            } else {
                ty.clone()
            }
        }
        Type::App(name, _) if name == adt_name => Type::TyVar(mu_var.to_string()),
        _ => ty.clone(),
    }
}

/// Structurally compare a template type with a concrete type, extracting
/// param→concrete mappings where the template has TyVar(param).
fn extract_mappings_structural(
    template: &tungsten_core::types::Type,
    concrete: &tungsten_core::types::Type,
    params: &[String],
    subst: &mut std::collections::HashMap<String, tungsten_core::types::Type>,
) {
    use tungsten_core::types::Type;

    match (template, concrete) {
        (Type::TyVar(name), concrete_ty) if params.contains(name) => {
            // Template position is a type parameter — extract mapping.
            // Skip no-op mappings (param → TyVar(param)).
            let is_noop = matches!(concrete_ty, Type::TyVar(n) if n == name);
            if is_noop {
                return;
            }
            // Prefer non-TyVar (fully concrete) mappings over TyVar-based ones.
            let should_insert = subst.get(name).map_or(true, |existing| {
                let existing_is_tyvar = matches!(existing, Type::TyVar(n) if n == name);
                let existing_is_any_tyvar = matches!(existing, Type::TyVar(_));
                let new_is_tyvar = matches!(concrete_ty, Type::TyVar(_));
                // Always replace no-ops; prefer non-TyVar over TyVar
                existing_is_tyvar || (existing_is_any_tyvar && !new_is_tyvar)
            });
            if should_insert {
                subst.insert(name.clone(), concrete_ty.clone());
            }
        }
        (Type::Arrow(t1, t2), Type::Arrow(c1, c2))
        | (Type::Product(t1, t2), Type::Product(c1, c2))
        | (Type::Sum(t1, t2), Type::Sum(c1, c2)) => {
            extract_mappings_structural(t1, c1, params, subst);
            extract_mappings_structural(t2, c2, params, subst);
        }
        (Type::Mu(_, tbody), Type::Mu(_, cbody)) => {
            extract_mappings_structural(tbody, cbody, params, subst);
        }
        (Type::Adt(_, _, tvariants), Type::Adt(_, _, cvariants)) => {
            for (tv, cv) in tvariants.iter().zip(cvariants.iter()) {
                extract_mappings_structural(&tv.1, &cv.1, params, subst);
            }
        }
        _ => {}
    }
}
