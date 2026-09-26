//! Tests for the elaborator's two poison refusals (ADR 7.8.26d §2.2).

use super::*;
use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind, ValueDef};
use crate::span::Span;
use tungsten_core::terms::{SpannedTerm, Term};
use tungsten_core::{Context, Type};

fn core_def(name: &str, ty: Type) -> CoreDef {
    CoreDef {
        name: name.to_string(),
        ty,
        term: SpannedTerm {
            term: Term::Unit,
            span: None,
        },
        span: Span::default(),
    }
}

fn value(name: &str, ty: Type) -> (String, ValueDef) {
    (
        name.to_string(),
        ValueDef {
            name: name.to_string(),
            ty,
            visibility: crate::ast::Visibility::Public,
            span: Span::default(),
        },
    )
}

fn ty_def(name: &str, kind: TypeDefKind) -> (String, TypeDef) {
    (
        name.to_string(),
        TypeDef {
            name: name.to_string(),
            params: vec![],
            kind,
            visibility: crate::ast::Visibility::Public,
            span: Span::default(),
            defining_module: None,
            encoded_type: None,
            field_visibilities: vec![],
        },
    )
}

// ─────────────────────────────────────────────────────────────────────────
// admit_core_def — the item-output guard
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_clean_definition_is_admitted() {
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    let admitted = elab.admit_core_def(core_def("f", Type::arrow(Type::Nat, Type::Nat)));
    assert!(admitted.is_some());
    assert!(elab.errors.is_empty());
}

#[test]
fn a_poisoned_definition_is_refused() {
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    let admitted = elab.admit_core_def(core_def("f", Type::Error));
    assert!(admitted.is_none(), "poison must not reach Core/CIR");
}

#[test]
fn poison_nested_in_a_signature_is_refused() {
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    let ty = Type::arrow(Type::Error, Type::Nat);
    assert!(elab.admit_core_def(core_def("f", ty)).is_none());
}

/// A refusal with no error already recorded would silently *drop* a
/// definition. It must record its own diagnostic so the run still fails.
#[test]
fn a_silent_producer_is_reported() {
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    elab.admit_core_def(core_def("f", Type::Error));
    assert_eq!(elab.errors.len(), 1);
    assert!(elab.errors[0].to_string().contains('f'));
}

/// When the producer *did* record its error, the refusal adds nothing — the
/// user sees the root cause, not an internal-error postscript.
#[test]
fn a_reported_producer_adds_no_second_error() {
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    elab.record_error(ElabError::new(
        Span::default(),
        ElabErrorKind::Other("root cause".to_string()),
    ));
    elab.admit_core_def(core_def("f", Type::Error));
    assert_eq!(elab.errors.len(), 1);
    assert!(elab.errors[0].to_string().contains("root cause"));
}

// ─────────────────────────────────────────────────────────────────────────
// first_poisoned_export — the cache-writer guard
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn clean_exports_pass() {
    let exports = ModuleExports {
        types: vec![ty_def(
            "Point",
            TypeDefKind::Record(vec![("x".into(), Type::Nat)]),
        )],
        values: vec![value("f", Type::arrow(Type::Nat, Type::Nat))],
        constructors: vec![],
    };
    assert_eq!(first_poisoned_export(&exports), None);
}

#[test]
fn empty_exports_pass() {
    assert_eq!(first_poisoned_export(&ModuleExports::default()), None);
}

#[test]
fn a_poisoned_value_signature_is_caught() {
    let exports = ModuleExports {
        values: vec![value("f", Type::arrow(Type::Error, Type::Nat))],
        ..ModuleExports::default()
    };
    assert_eq!(first_poisoned_export(&exports), Some("f".to_string()));
}

#[test]
fn a_poisoned_alias_body_is_caught() {
    let exports = ModuleExports {
        types: vec![ty_def("Bad", TypeDefKind::Alias(Type::Error))],
        ..ModuleExports::default()
    };
    assert_eq!(first_poisoned_export(&exports), Some("Bad".to_string()));
}

#[test]
fn a_poisoned_record_field_is_caught() {
    let exports = ModuleExports {
        types: vec![ty_def(
            "Bad",
            TypeDefKind::Record(vec![("ok".into(), Type::Nat), ("bad".into(), Type::Error)]),
        )],
        ..ModuleExports::default()
    };
    assert_eq!(first_poisoned_export(&exports), Some("Bad".to_string()));
}

#[test]
fn a_poisoned_constructor_field_is_caught() {
    let ctor = Constructor {
        name: "Mk".to_string(),
        fields: vec![Type::Nat, Type::Error],
        index: 0,
        visibility: None,
        span: Span::default(),
    };
    let exports = ModuleExports {
        types: vec![ty_def("Bad", TypeDefKind::ADT(vec![ctor]))],
        ..ModuleExports::default()
    };
    assert_eq!(first_poisoned_export(&exports), Some("Bad".to_string()));
}

/// The type-body producer (D2) poisons `encoded_type` while leaving the
/// `Stub` kind in place, so a `Stub` whose encoding is poison must be caught
/// even though `Stub` itself carries no type.
#[test]
fn a_poisoned_encoding_on_a_stub_is_caught() {
    let (name, mut def) = ty_def("Bad", TypeDefKind::Stub);
    def.encoded_type = Some(Type::Error);
    let exports = ModuleExports {
        types: vec![(name, def)],
        ..ModuleExports::default()
    };
    assert_eq!(first_poisoned_export(&exports), Some("Bad".to_string()));
}

#[test]
fn a_clean_stub_passes() {
    let exports = ModuleExports {
        types: vec![ty_def("Pending", TypeDefKind::Stub)],
        ..ModuleExports::default()
    };
    assert_eq!(first_poisoned_export(&exports), None);
}

/// Values are scanned before types, but a poisoned type alone must still be
/// found — a `find` on the wrong collection would pass this only by luck.
#[test]
fn a_poisoned_type_is_found_when_values_are_clean() {
    let exports = ModuleExports {
        types: vec![
            ty_def("Fine", TypeDefKind::Alias(Type::Nat)),
            ty_def("Bad", TypeDefKind::Alias(Type::Ptr(Box::new(Type::Error)))),
        ],
        values: vec![value("f", Type::Nat)],
        constructors: vec![],
    };
    assert_eq!(first_poisoned_export(&exports), Some("Bad".to_string()));
}
