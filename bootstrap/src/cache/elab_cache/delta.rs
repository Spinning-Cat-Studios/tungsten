//! Delta exports — what a module adds that its predecessors did not.
//!
//! Both cache entry kinds (`CachedModuleSignature`, `CachedModuleFullOutput`)
//! store only this difference rather than the full accumulated environment,
//! which is what keeps an entry at ~200B–2KB instead of ~1.8MB (ADR 10.5.26n).
//!
//! It is also the single choke point every export passes through on its way to
//! disk, which is why the poison refusal lives here rather than at each
//! `from_output` (ADR 7.8.26d §2.2).

use crate::elaborate::ModuleExports;

/// Compute delta exports: entries in `full` that are NOT in `prior`.
///
/// Refuses poison on the way past. A cached `Type::Error` would be *durable*: a
/// signature poisoned only because this run failed, reloaded by a later run as
/// if it were a real type. The route is closed today — exports are built only
/// on a clean run — and this keeps it closed if that ever changes.
pub(super) fn compute_delta_exports(full: &ModuleExports, prior: &ModuleExports) -> ModuleExports {
    use std::collections::HashSet;
    let prior_types: HashSet<&str> = prior.types.iter().map(|(n, _)| n.as_str()).collect();
    let prior_values: HashSet<&str> = prior.values.iter().map(|(n, _)| n.as_str()).collect();
    let prior_ctors: HashSet<&str> = prior.constructors.iter().map(|(n, _)| n.as_str()).collect();

    let delta = ModuleExports {
        types: full
            .types
            .iter()
            .filter(|(n, _)| !prior_types.contains(n.as_str()))
            .cloned()
            .collect(),
        values: full
            .values
            .iter()
            .filter(|(n, _)| !prior_values.contains(n.as_str()))
            .cloned()
            .collect(),
        constructors: full
            .constructors
            .iter()
            .filter(|(n, _)| !prior_ctors.contains(n.as_str()))
            .cloned()
            .collect(),
    };

    if let Some(name) = crate::elaborate::first_poisoned_export(&delta) {
        panic!(
            "internal error: the poisoned export `{name}` reached the elaboration \
             cache writer. Exports are built only on a clean run, so this means \
             export construction was decoupled from error state (ADR 7.8.26d §2.2)."
        );
    }

    delta
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Visibility;
    use crate::elaborate::{TypeDef, TypeDefKind, ValueDef};
    use crate::span::Span;
    use tungsten_core::Type;

    fn value(name: &str) -> (String, ValueDef) {
        (
            name.to_string(),
            ValueDef {
                name: name.to_string(),
                ty: Type::Nat,
                visibility: Visibility::Public,
                span: Span::default(),
            },
        )
    }

    fn ty(name: &str) -> (String, TypeDef) {
        (
            name.to_string(),
            TypeDef {
                name: name.to_string(),
                params: vec![],
                kind: TypeDefKind::Alias(Type::Nat),
                visibility: Visibility::Public,
                span: Span::default(),
                defining_module: None,
                encoded_type: None,
                field_visibilities: vec![],
            },
        )
    }

    #[test]
    fn everything_is_new_when_there_is_no_prior() {
        let full = ModuleExports {
            values: vec![value("f"), value("g")],
            ..ModuleExports::default()
        };
        let delta = compute_delta_exports(&full, &ModuleExports::default());
        assert_eq!(delta.values.len(), 2);
    }

    #[test]
    fn entries_already_in_prior_are_dropped() {
        let full = ModuleExports {
            values: vec![value("f"), value("g")],
            ..ModuleExports::default()
        };
        let prior = ModuleExports {
            values: vec![value("f")],
            ..ModuleExports::default()
        };
        let delta = compute_delta_exports(&full, &prior);
        assert_eq!(delta.values.len(), 1);
        assert_eq!(delta.values[0].0, "g");
    }

    /// The three collections are filtered against three *separate* prior sets —
    /// a copy-paste that checked values against `prior_types` would pass any
    /// single-collection test.
    #[test]
    fn each_collection_is_filtered_against_its_own_prior_set() {
        let full = ModuleExports {
            types: vec![ty("A"), ty("B")],
            values: vec![value("A"), value("c")],
            constructors: vec![],
        };
        // "A" is prior as a *type* only; the value "A" must survive.
        let prior = ModuleExports {
            types: vec![ty("A")],
            ..ModuleExports::default()
        };
        let delta = compute_delta_exports(&full, &prior);
        assert_eq!(delta.types.len(), 1, "type A was prior");
        assert_eq!(delta.types[0].0, "B");
        assert_eq!(delta.values.len(), 2, "value A is not the same name-space");
    }

    #[test]
    fn nothing_new_yields_an_empty_delta() {
        let full = ModuleExports {
            values: vec![value("f")],
            ..ModuleExports::default()
        };
        let delta = compute_delta_exports(&full, &full.clone());
        assert!(delta.values.is_empty());
    }
}
