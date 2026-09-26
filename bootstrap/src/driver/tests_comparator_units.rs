//! Synthetic comparator codegen units (ADR 29.6.26f §T11.8).
//!
//! The *concrete* `__compare` path emits `Global("compare_T")` directly, so the
//! definitions it calls have to be manufactured before codegen. A pass that
//! quietly produced nothing would leave every one of those calls unresolved at
//! link time, which is why the reach is asserted rather than assumed.
//!
//! These read and write the process-global request registry, so each takes
//! `requests::TEST_EXCLUSIVE` — the guard declared beside the registry itself.
//! Without it they race any test that clears it (ADR 11.8.26b added one), and
//! the loser reads an empty registry and reports "nothing was synthesized".
//!
//! Tests: bootstrap/src/driver/mod.rs

use tungsten_core::terms::SpannedTerm;
use tungsten_core::{Term, Type};

use super::synthesized_comparator_units;
use crate::comparator::{requests, ComparatorTypes};
use crate::elaborate::CoreDef;
use crate::span::Span;

/// A def whose body calls `symbol` — the shape `elab_compare` emits.
fn def_calling(name: &str, symbol: &str) -> CoreDef {
    CoreDef {
        name: name.to_string(),
        ty: Type::Nat,
        term: SpannedTerm::generated(Term::app(Term::Global(symbol.to_string()), Term::Zero)),
        span: Span::new(0, 0),
    }
}

#[test]
fn a_referenced_comparator_becomes_its_own_codegen_unit() {
    let _guard = requests::TEST_EXCLUSIVE.lock().unwrap();
    // The registry is process-global, so the symbol is registered here rather
    // than inherited from whatever ran before.
    requests::register("compare_Nat".to_string(), Type::Nat);
    let defs = vec![def_calling("uses_compare", "compare_Nat")];

    let units = synthesized_comparator_units(defs.iter(), false, &ComparatorTypes::default());

    assert!(
        !units.is_empty(),
        "a referenced-but-undefined comparator must be synthesized, or the \
         call is unresolved at link time"
    );
    assert!(
        units
            .iter()
            .any(|u| u.defs.iter().any(|d| d.name == "compare_Nat")),
        "the synthesized unit must define the symbol that was referenced"
    );
    assert!(
        units.iter().all(|u| u.module_path == vec!["__comparator"]),
        "synthesized units live in their own module path"
    );
}

#[test]
fn a_comparator_that_is_already_defined_is_not_synthesized_again() {
    let _guard = requests::TEST_EXCLUSIVE.lock().unwrap();
    // The non-vacuity twin: a pass that emitted a unit for every reference
    // would pass the test above and duplicate every symbol.
    requests::register("compare_Nat".to_string(), Type::Nat);
    let defs = vec![
        def_calling("uses_compare", "compare_Nat"),
        CoreDef {
            name: "compare_Nat".to_string(),
            ty: Type::Nat,
            term: SpannedTerm::generated(Term::Zero),
            span: Span::new(0, 0),
        },
    ];

    let units = synthesized_comparator_units(defs.iter(), false, &ComparatorTypes::default());

    assert!(
        units.is_empty(),
        "an already-defined comparator must not be emitted a second time"
    );
}

#[test]
fn defs_that_reference_no_comparator_synthesize_nothing() {
    let _guard = requests::TEST_EXCLUSIVE.lock().unwrap();
    let defs = vec![def_calling("plain", "some_other_global")];
    let units = synthesized_comparator_units(defs.iter(), false, &ComparatorTypes::default());
    assert!(units.is_empty());
}
