//! Expanding a **named ADT application** to the encoding it stands for
//! (ADR 1.8.26c).
//!
//! A field typed `List<TypeParam>` arrives at comparator synthesis as
//! `Type::App("List", [TypeParam])`. Until this module existed, every site that
//! resolved a named type required `args.is_empty()`, so an instantiation fell
//! through to the opaque-leaf arm and the enclosing type was reported
//! noncomparable — the sole remaining blocker on ADR 29.6.26f AC 2 / AC 7.
//!
//! ## Why `adt_types`, and why a new producer
//!
//! A generic ADT has **no monomorphic stored encoding**: `encoded_types` holds
//! only zero-parameter types (`elaborate::codegen_types`), which is the same
//! fact [`ComparatorTypes`](super::context::ComparatorTypes) relies on when it
//! deliberately leaves `α_List` unresolvable. `TypeProvenance`'s `AdtOrigin`
//! carries constructor *names* only. `adt_types` — `name → (params,
//! constructors)` — is the only map that carries generic definitions.
//!
//! The canonical stored encoder is `Elaborator::encode_adt_type`, which is
//! `&mut self` over `self.env`. Comparator synthesis runs from a
//! `ProjectOutput` at gate time, where no `Elaborator` exists, so it cannot be
//! called. What this module does instead is drive the encoder's **own leaf
//! builders**, not copies of them:
//!
//! | Step | Producer invoked | Owner |
//! |---|---|---|
//! | self/group reference → `α_Name` | `replace_self_reference` | `elaborate::types::resolve_refs` |
//! | parameter → argument | `Type::substitute` | `tungsten_core` |
//! | fields → payload product | `ctor_fields_product` | `elaborate::types::encoding` |
//! | payloads → sum/ADT body | `build_adt_sum_body` | `elaborate::types::encoding` |
//! | recursive → μ-binder chain | `Type::mu` | `tungsten_core` |
//!
//! So the *driver* is new and the *shapes* are not. That distinction is what
//! ADR 1.8.26c AC 5 pins, in both halves: a zero-parameter expansion must equal
//! `project.encoded_types[name]` structurally (the encode arm), and two
//! instantiations of one generic must differ **only** at the argument slots
//! (the substitute arm). This tree has been burned before by a parallel encoder
//! that looked right — `encoding_utils.rs` records the third copy, whose
//! right-nested products silently failed `types_pattern_match`, and ADR
//! 21.7.26e found three more on the normalize side.
//!
//! ## The two fidelity rules that are not obvious
//!
//! **Mutual-group members become binders, not references.** A cluster encodes
//! as one nested μ-binder per SCC member, all wrapping the entry member's body
//! (ADR 18.4.26i; `super::context`), so a reference to a sibling is rewritten to
//! that sibling's binder rather than inlined. Skipping this would leave a bare
//! `TyVar("Stmt")` where the encoder wrote `TyVar("α_Stmt")` — a shape the
//! support predicate then refuses, so the omission would show up as a
//! *comparability* regression, not merely a cosmetic difference.
//!
//! **A reference to another ADT is inlined from its stored encoding.** That is
//! what `resolve_type_references_impl` does during encoding, and it is why
//! `Stmt`'s stored encoding carries `Adt("Item", [], […])` in full. Records are
//! deliberately **not** inlined: they stay nominal `TyVar("Span")` in stored
//! encodings, and `records()` resolves them downstream.
//!
//! ## Where the expansion stops
//!
//! A field that is itself an application is expanded too — `type Boxed =
//! B(List<Nat>)` stores the *inlined* `μα_List. …`, not `App("List", [Nat])`,
//! so an expander that stopped at the outermost application would disagree with
//! the encoder on every type with a generic field. What bounds it is the
//! encoder's own rule, not a depth counter: a name already being expanded is a
//! **cycle break**, and `check_encoding_cycle` emits `App(name, args)` verbatim
//! for a parameterized one. So `A<T> = MkA(B<T>)` / `B<T> = MkB(A<T>)` settles
//! after two rounds with an `App` at the join, which the closure walk then
//! resolves — under its own bound, since polymorphic recursion can keep
//! producing *fresh* applications that no name stack repeats.

use std::collections::{HashMap, HashSet};

use tungsten_core::Type;

use crate::driver::AdtTypes;
use crate::elaborate::types::encoding::{build_adt_sum_body, ctor_fields_product};
use crate::elaborate::types::resolve_refs::replace_self_reference;

/// The maps an expansion resolves against, bundled so the walk stays under the
/// parameter cap and so a future input cannot be added to one caller only.
pub(super) struct AdtDefinitions<'a> {
    /// `name → (params, constructors)` — the only map carrying generic bodies.
    pub adts: &'a AdtTypes,
    /// Stored (Encoding Finalization) encodings, for inlining a reference to
    /// another ADT the way the canonical encoder does.
    pub encoded: &'a HashMap<String, Type>,
    /// SCC membership, so a sibling reference becomes that sibling's μ-binder.
    pub groups: &'a HashMap<String, Vec<String>>,
}

/// Expand `name<args>` to the sum/μ type the stored encoder would have built
/// for it, or `None` when `name` is not a known ADT.
///
/// `args` may be empty — `App("Foo", [])` is a legitimate zero-parameter
/// application, and AC 5's encode arm compares exactly that against the stored
/// encoding. The comparator's *synthesis* arm only reaches here with a
/// non-empty `args`, because a zero-argument application still resolves through
/// the record path first.
///
/// Arity is **not** checked: `params.iter().zip(args)` substitutes the pairs it
/// has. An under-applied generic leaves its remaining parameters as free
/// `TyVar`s, which the support predicate then refuses by path — a report,
/// rather than a silently wrong shape.
#[must_use]
pub(super) fn expand_application(name: &str, args: &[Type], defs: &AdtDefinitions) -> Option<Type> {
    expand_under(name, args, defs, &HashSet::new())
}

/// [`expand_application`] with the set of ADT names already being expanded on
/// this path, so a re-entry becomes the encoder's cycle break rather than a
/// non-terminating inline.
fn expand_under(
    name: &str,
    args: &[Type],
    defs: &AdtDefinitions,
    open: &HashSet<String>,
) -> Option<Type> {
    let (params, constructors) = defs.adts.get(name)?;
    let mut open = open.clone();
    open.insert(name.to_string());
    let self_binder = mu_binder(name);
    let sibling_binders = sibling_binders(name, defs.groups);
    let is_recursive = !sibling_binders.is_empty()
        || constructors
            .iter()
            .flat_map(|ctor| ctor.fields.iter())
            .any(|field| names_the_adt(field, name));

    let payloads: Vec<Type> = constructors
        .iter()
        .map(|ctor| {
            let fields = ctor
                .fields
                .iter()
                .map(|field| {
                    let bound = bind_recursive_references(
                        field,
                        BindCtx {
                            adt_name: name,
                            self_binder: &self_binder,
                            is_recursive,
                            sibling_binders: &sibling_binders,
                        },
                    );
                    let substituted = substitute_params(&bound, params, args);
                    // `adt_types` holds constructor fields exactly as collection
                    // left them, so a record reference can still arrive carrying
                    // the Type-Body Collection `@`-prefix. Stored encodings have
                    // none and `records()` is keyed without one, so an `@` that
                    // survives is reported as `@Ident is not defined` — about a
                    // record the project defines. `ComparatorTypes::new` strips
                    // its inputs for the same reason; this covers the one input
                    // that arrives raw.
                    inline_adt_references(&substituted, defs, &open).strip_tyvar_at_prefix()
                })
                .collect();
            ctor_fields_product(fields)
        })
        .collect();

    let body = build_adt_sum_body(payloads, constructors, name, args);
    if !is_recursive {
        return Some(body);
    }
    // One binder per SCC member, siblings innermost, self outermost — the
    // `finalize_adt_encoding` order.
    let wrapped = sibling_binders
        .iter()
        .rev()
        .fold(body, |acc, (_, binder)| Type::mu(binder, acc));
    Some(Type::mu(&self_binder, wrapped))
}

/// What [`bind_recursive_references`] needs to know about the ADT it is
/// rewriting fields for.
struct BindCtx<'a> {
    adt_name: &'a str,
    self_binder: &'a str,
    is_recursive: bool,
    sibling_binders: &'a [(String, String)],
}

/// Rewrite every recursive occurrence in a field to the binder that stands for
/// it: the ADT's own name to its binder, then each SCC sibling to that
/// sibling's.
///
/// Runs **before** parameter substitution so that a self-reference spelled with
/// arguments (`List<T>` inside `List<T>`) collapses to the binder before
/// substitution could rewrite its argument into something that no longer looks
/// like a self-reference.
fn bind_recursive_references(field: &Type, ctx: BindCtx) -> Type {
    let mut result = if ctx.is_recursive {
        replace_self_reference(field, ctx.adt_name, ctx.self_binder)
    } else {
        field.clone()
    };
    for (sibling, binder) in ctx.sibling_binders {
        result = replace_self_reference(&result, sibling, binder);
    }
    result
}

/// Substitute the ADT's type parameters with the application's arguments.
fn substitute_params(ty: &Type, params: &[String], args: &[Type]) -> Type {
    params
        .iter()
        .zip(args.iter())
        .fold(ty.clone(), |acc, (param, arg)| acc.substitute(param, arg))
}

/// Replace a reference to *another* ADT with the encoding it stands for, the
/// way `resolve_type_references_impl` does during encoding.
///
/// A zero-argument reference is served from the **stored** encoding — already
/// closed, so it is spliced in verbatim and never walked again. A parameterized
/// one has no stored encoding and is expanded on the spot, unless its name is
/// already open on this path, in which case it is left as the `App` the encoder
/// itself emits as a cycle break.
///
/// A **record** name is deliberately left alone: stored encodings keep records
/// nominal (`TyVar("Span")`), and `ComparatorTypes::records()` resolves them.
fn inline_adt_references(ty: &Type, defs: &AdtDefinitions, open: &HashSet<String>) -> Type {
    let bare = |name: &str| name.strip_prefix('@').unwrap_or(name).to_string();
    match ty {
        Type::TyVar(name) => stored_encoding(&bare(name), defs).unwrap_or_else(|| ty.clone()),
        Type::App(name, args) if args.is_empty() => {
            stored_encoding(&bare(name), defs).unwrap_or_else(|| ty.clone())
        }
        Type::App(name, args) if !open.contains(&bare(name)) => {
            let inlined: Vec<Type> = args
                .iter()
                .map(|arg| inline_adt_references(arg, defs, open))
                .collect();
            expand_under(&bare(name), &inlined, defs, open)
                .unwrap_or_else(|| Type::app(name.clone(), inlined))
        }
        _ => ty.map_children(|child| inline_adt_references(child, defs, open)),
    }
}

/// The stored encoding of a zero-parameter **ADT** (never a record).
fn stored_encoding(name: &str, defs: &AdtDefinitions) -> Option<Type> {
    defs.adts.contains_key(name).then_some(())?;
    defs.encoded.get(name).cloned()
}

/// The μ-binder convention: `α_<AdtName>`.
fn mu_binder(name: &str) -> String {
    format!("α_{name}")
}

/// The SCC siblings of `name` and their binders, in the encoder's order (the
/// group vector minus the type itself).
fn sibling_binders(name: &str, groups: &HashMap<String, Vec<String>>) -> Vec<(String, String)> {
    groups
        .get(name)
        .map(|group| {
            group
                .iter()
                .filter(|member| *member != name)
                .map(|member| (member.clone(), mu_binder(member)))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `ty` mentions `name` in any of the three spellings a self-reference
/// can take (bare `TyVar`, `@`-prefixed `TyVar`, or an `App`/`Adt` head) —
/// the recursiveness test, matched to what `replace_self_reference` rewrites so
/// the two cannot disagree about whether a μ-binder is needed.
fn names_the_adt(ty: &Type, name: &str) -> bool {
    match ty {
        Type::TyVar(v) => v == name || v.strip_prefix('@') == Some(name),
        Type::App(head, _) | Type::Adt(head, _, _) if head == name => true,
        _ => ty.children().iter().any(|child| names_the_adt(child, name)),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod encoder_agreement;
