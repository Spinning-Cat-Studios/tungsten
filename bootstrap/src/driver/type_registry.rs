//! Type name registry for reverse-looking up user-defined type names from Core encodings.
//!
//! Maintains thread-local registries that map core type encodings back to their
//! user-defined names (exact match for non-parameterized types, pattern match
//! for parameterized types like `Option<T>`, `List<T>`).

use std::cell::RefCell;
use std::collections::HashMap;

use tungsten_core::Type;

// ============================================================================
// Type Name Registry (exact match for non-parameterized types)
// ============================================================================

// Thread-local registry mapping encoded types to their user-defined names.
// This enables reverse lookup during error formatting without threading Env everywhere.
// Uses Vec instead of HashMap because Type doesn't implement Hash.
// The list is small (only non-parameterized types) so linear search is fine.
thread_local! {
    static TYPE_NAME_REGISTRY: RefCell<Vec<(Type, String)>> = const { RefCell::new(Vec::new()) };
}

/// Register a type-to-name mapping for reverse lookup in error messages.
///
/// Called during elaboration Phase 1e for non-parameterized types.
pub fn register_type_name(encoded_type: Type, name: String) {
    TYPE_NAME_REGISTRY.with(|registry| {
        let mut reg = registry.borrow_mut();
        // Avoid duplicates
        if !reg.iter().any(|(ty, _)| ty == &encoded_type) {
            reg.push((encoded_type, name));
        }
    });
}

/// Clear the type name registry (for testing or new compilation units).
pub fn clear_type_name_registry() {
    TYPE_NAME_REGISTRY.with(|registry| {
        registry.borrow_mut().clear();
    });
    TYPE_PATTERN_REGISTRY.with(|registry| {
        registry.borrow_mut().clear();
    });
}

/// Look up a user-defined type name from its Core encoding (exact match).
///
/// Returns Some(name) if a non-parameterized type matches exactly.
fn lookup_type_name_exact(ty: &Type) -> Option<String> {
    TYPE_NAME_REGISTRY.with(|registry| {
        registry
            .borrow()
            .iter()
            .find(|(encoded, _)| encoded == ty)
            .map(|(_, name)| name.clone())
    })
}

// ============================================================================
// Type Pattern Registry (pattern match for parameterized types)
// ============================================================================

/// A type pattern for reverse lookup of parameterized types.
///
/// For `Option<T>`, the pattern would be `Unit + TyVar("T")`.
/// For `List<T>`, the pattern would be `μα_List. Unit + (TyVar("T") × TyVar("α_List"))`.
#[derive(Debug, Clone)]
pub struct TypePattern {
    /// Type name (e.g., "Option", "List")
    pub name: String,

    /// Parameter names in order (e.g., ["T"])
    pub params: Vec<String>,

    /// The pattern type with parameters as TyVar placeholders
    pub pattern: Type,

    /// The μ-variable name if this is a recursive type (e.g., "α_List")
    pub mu_var: Option<String>,
}

thread_local! {
    pub(crate) static TYPE_PATTERN_REGISTRY: RefCell<Vec<TypePattern>> = const { RefCell::new(Vec::new()) };
}

/// Register a type pattern for reverse lookup of parameterized types.
///
/// Called during elaboration Phase 1e for parameterized types like `Option<T>`.
pub fn register_type_pattern(pattern: TypePattern) {
    TYPE_PATTERN_REGISTRY.with(|registry| {
        let mut reg = registry.borrow_mut();
        // Avoid duplicates
        if !reg.iter().any(|p| p.name == pattern.name) {
            reg.push(pattern);
        }
    });
}

/// Try to match a concrete type against a pattern, returning variable bindings.
///
/// For example, matching `Unit + Nat` against `Unit + TyVar("T")` returns `{"T": Nat}`.
fn try_match_pattern<'a>(
    concrete: &Type,
    pattern: &'a Type,
    bindings: &mut HashMap<&'a str, Type>,
    mu_var: Option<&str>,
) -> bool {
    match (concrete, pattern) {
        // Base types must match exactly
        (Type::Nat, Type::Nat)
        | (Type::Bool, Type::Bool)
        | (Type::Unit, Type::Unit)
        | (Type::Void, Type::Void)
        | (Type::Prop, Type::Prop)
        | (Type::String, Type::String) => true,

        // Pattern variable: bind or check consistency
        (concrete, Type::TyVar(v)) => {
            // If this is the μ-variable, handle specially
            if Some(v.as_str()) == mu_var {
                // In concrete, this should also be a TyVar (the μ-bound variable)
                matches!(concrete, Type::TyVar(_))
            } else {
                // Regular type parameter - bind it
                if let Some(existing) = bindings.get(v.as_str()) {
                    // Must match existing binding
                    concrete == existing
                } else {
                    // New binding
                    bindings.insert(v.as_str(), concrete.clone());
                    true
                }
            }
        }

        // Binary structural matching
        (Type::Sum(ca, cb), Type::Sum(pa, pb))
        | (Type::Product(ca, cb), Type::Product(pa, pb))
        | (Type::Arrow(ca, cb), Type::Arrow(pa, pb)) => {
            try_match_pattern(ca, pa, bindings, mu_var)
                && try_match_pattern(cb, pb, bindings, mu_var)
        }

        // Binding types: match structurally, tracking the bound variable
        (Type::Mu(cv, cbody), Type::Mu(pv, pbody))
        | (Type::Forall(cv, cbody), Type::Forall(pv, pbody)) => {
            let _ = cv; // concrete's var name (unused, we match structurally)
            try_match_pattern(cbody, pbody, bindings, Some(pv.as_str()))
        }

        // Pointer and Ref types
        (Type::Ptr(ci), Type::Ptr(pi)) | (Type::Ref(ci), Type::Ref(pi)) => {
            try_match_pattern(ci, pi, bindings, mu_var)
        }

        // Eq types (rare in user code, but handle for completeness)
        (Type::Eq(cty, ct1, ct2), Type::Eq(pty, pt1, pt2)) => {
            try_match_pattern(cty, pty, bindings, mu_var) && ct1 == pt1 && ct2 == pt2
        }

        // No match
        _ => false,
    }
}

/// Try to match a concrete type against all registered patterns.
///
/// Returns (type_name, type_args) if a pattern matches.
///
/// To avoid false positives (e.g., Stmt matching as Result because both are sums),
/// we ONLY pattern match for Mu types where the μ-variable name matches a registered
/// pattern name. This is conservative but prevents misleading error messages.
fn lookup_type_by_pattern(ty: &Type) -> Option<(String, Vec<Type>)> {
    TYPE_PATTERN_REGISTRY.with(|registry| {
        let patterns = registry.borrow();

        // Only match Mu types where the μ-variable name matches a pattern name.
        // This prevents Sum types like Stmt from being misidentified as Result.
        if let Type::Mu(mu_var, _) = ty {
            if let Some(type_name) = mu_var.strip_prefix("α_") {
                for pattern in patterns.iter() {
                    if pattern.name == type_name {
                        let mut bindings: HashMap<&str, Type> = HashMap::new();
                        if try_match_pattern(
                            ty,
                            &pattern.pattern,
                            &mut bindings,
                            pattern.mu_var.as_deref(),
                        ) {
                            let args: Option<Vec<Type>> = pattern
                                .params
                                .iter()
                                .map(|p| bindings.get(p.as_str()).cloned())
                                .collect();
                            if let Some(args) = args {
                                return Some((pattern.name.clone(), args));
                            }
                        }
                    }
                }
            }
        }

        // NOTE: We intentionally do NOT fall back to structural matching for non-Mu types.
        // Structural matching is too error-prone - types like Stmt (a sum type) would
        // incorrectly match Result's pattern (also a sum type), leading to confusing
        // error messages like "expected List<Stmt>, found List<Result<...>>".
        None
    })
}

/// Look up a user-defined type name from its Core encoding.
///
/// Tries exact match first (for non-parameterized types),
/// then pattern match (for parameterized types like `Option<T>`).
pub(crate) fn lookup_type_name(ty: &Type) -> Option<String> {
    // First try exact match (non-parameterized types)
    if let Some(name) = lookup_type_name_exact(ty) {
        return Some(name);
    }

    // Then try pattern match (parameterized types)
    if let Some((name, args)) = lookup_type_by_pattern(ty) {
        if args.is_empty() {
            return Some(name);
        }
        // Format as Name<Arg1, Arg2, ...>
        // Recursively format args in case they're also user types
        let formatted_args: Vec<String> = args
            .iter()
            .map(|arg| {
                // Try to look up the arg too (handles nested user types)
                lookup_type_name(arg).unwrap_or_else(|| format_type_simple(arg))
            })
            .collect();
        return Some(format!("{}<{}>", name, formatted_args.join(", ")));
    }

    None
}

/// Simple type formatting without depth limit (for type arguments).
fn format_type_simple(ty: &Type) -> String {
    match ty {
        Type::Unit => "Unit".to_string(),
        Type::Void => "Void".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::Nat => "Nat".to_string(),
        Type::Prop => "Prop".to_string(),
        Type::String => "String".to_string(),
        Type::TyVar(name) => name.strip_prefix('@').unwrap_or(name).to_string(),
        Type::Arrow(a, b) => format!("{} -> {}", format_type_simple(a), format_type_simple(b)),
        Type::Product(a, b) => format!("({} × {})", format_type_simple(a), format_type_simple(b)),
        Type::Sum(a, b) => format!("({} + {})", format_type_simple(a), format_type_simple(b)),
        _ => format!("{:?}", ty),
    }
}
