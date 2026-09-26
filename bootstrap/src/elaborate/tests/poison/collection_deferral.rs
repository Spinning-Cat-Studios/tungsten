//! The collection pass's conditional deferral (ADR 14.8.26g D2 as
//! tightened at P3): only a fully poison-compensated error set defers;
//! anything else short-circuits as it did before the deferral existed.

use crate::elaborate::Elaborator;
use tungsten_core::{Context, Type};

/// The pass's verdict on a fixture: `Ok(deferred_count)` when it deferred
/// (errors left in place), `Err(returned_count)` when it short-circuited
/// (errors drained into the return). The two must be mutually exclusive —
/// an error appearing in both routes would be the D2a double-report.
fn collection_verdict(source: &str) -> Result<usize, usize> {
    let (ast, parse_errors) = crate::parse(source);
    assert!(
        parse_errors.is_empty(),
        "fixture must parse: {parse_errors:?}"
    );
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    match elab.run_collection_pass(&ast) {
        Ok(()) => {
            assert!(
                elab.collection_errors_all_poisoned(),
                "a deferred set must be fully poison-compensated"
            );
            Ok(elab.errors.len())
        }
        Err(returned) => {
            assert!(
                elab.errors.is_empty(),
                "a short-circuit must drain, not fork, the error list"
            );
            Err(returned.len())
        }
    }
}

/// A failed signature is poisoned by the D3 producer, so the whole set is
/// compensated and the pass defers into Pass 2.
#[test]
fn an_all_poisoned_error_set_defers() {
    assert_eq!(
        collection_verdict("fn f() -> NoSuchType { 0 }"),
        Ok(1),
        "a fully-compensated set must defer with its error kept"
    );
}

/// A duplicate definition leaves nothing missing — the first definition
/// stands — so no producer fires and the pass short-circuits. This is
/// what closes the unconditional P1b→P3 window.
#[test]
fn an_unpoisoned_error_set_still_short_circuits() {
    assert_eq!(
        collection_verdict("fn f() -> Nat { 0 }\nfn f() -> Nat { 1 }"),
        Err(1),
        "an unpoisoned set must short-circuit"
    );
}

/// One unpoisoned error in an otherwise-poisoned set still short-circuits
/// — the condition is EVERY error compensated, not any.
#[test]
fn a_mixed_error_set_still_short_circuits() {
    assert_eq!(
        collection_verdict("fn g() -> NoSuchType { 0 }\nfn f() -> Nat { 0 }\nfn f() -> Nat { 1 }",),
        Err(2),
        "a partially-poisoned set must short-circuit"
    );
}

/// The signature producer's registration itself: the failed name resolves
/// to `Type::Error` afterwards, which is what Pass 2 elaborates against.
#[test]
fn a_failed_signature_registers_poison() {
    let (ast, _) = crate::parse("fn f() -> NoSuchType { 0 }");
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    let _ = elab.run_collection_pass(&ast);
    let def = elab.env.values.get("f").expect("f must be registered");
    assert!(matches!(def.ty, Type::Error), "got {:?}", def.ty);
}

/// The type-body producer: a failed type body keeps its stub, poisoned, and
/// the compensation must make the set defer — an uncompensated count would
/// short-circuit this fixture.
#[test]
fn a_failed_type_body_poisons_its_stub() {
    let (ast, _) = crate::parse("type Broken = { field: NoSuchType }");
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    let verdict = elab.run_collection_pass(&ast);
    assert!(verdict.is_ok(), "a compensated type-body fault must defer");
    let def = elab.env.types.get("Broken").expect("stub must survive");
    assert!(
        matches!(def.encoded_type, Some(Type::Error)),
        "got {:?}",
        def.encoded_type
    );
}

/// The type-body producer also registers a poisoned ADT's constructor NAMES
/// (ADR 15.8.26d D2), with their source arity and index, so a call site
/// resolves to the poisoned parent rather than to an undefined value. An
/// entry already registered under that name is kept, not clobbered.
#[test]
fn a_failed_adt_body_registers_its_constructors_as_poisoned() {
    let (ast, _) = crate::parse("type Event = | Tick(NoSuchType, Nat) | Tock(Nat)");
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    // A prior registration under `Tock` with a deliberately odd index: the
    // producer must leave it alone rather than re-derive it from source.
    let placeholder = crate::elaborate::env::ConstructorInfo::test_stub("Event", 7, 1);
    elab.env
        .constructors
        .insert("Tock".to_string(), placeholder);
    assert!(elab.run_collection_pass(&ast).is_ok());

    let tick = elab
        .env
        .constructors
        .get("Tick")
        .expect("Tick must resolve");
    assert_eq!(
        (tick.type_name.as_str(), tick.index, tick.arity),
        ("Event", 0, 2)
    );
    let tock = elab
        .env
        .constructors
        .get("Tock")
        .expect("Tock must resolve");
    assert_eq!(tock.index, 7, "an existing registration is kept");
    assert!(elab.env.types["Event"].is_poison());
}

/// The global Signature Collection entry defers even an UNPOISONED error set
/// (ADR 14.8.26g D2 as built) — and reports it via `has_collection_errors`,
/// which is the only truth the deferral leaves a Pass-2-less caller.
#[test]
fn the_signature_collection_entry_defers_unpoisoned_errors() {
    let (ast, _) = crate::parse("use nonexistent_module::{missing_thing};\nfn f() -> Nat { 0 }");
    let mut ctx = Context::new();
    let collected = crate::elaborate::collect_definitions_for_signature_collection(
        &ast,
        &mut ctx,
        crate::driver::modules::ModuleInfo::default(),
        &crate::elaborate::ModuleExports::default(),
    )
    .expect("the global entry must defer, not short-circuit");
    assert!(
        collected.has_collection_errors(),
        "the deferred import error must be reported"
    );
}

/// A duplicate of a FINALIZED type (the import/cross-module shape — a
/// same-file duplicate of a local type is deliberately overwritable and
/// raises no error at all) is a fault of the second definition, not the
/// first: the producer must not poison the healthy definition that stands,
/// and the unpoisoned error short-circuits the pass.
#[test]
fn a_duplicate_of_a_finalized_type_does_not_poison_it() {
    use crate::elaborate::env::{TypeDef, TypeDefKind};
    let (ast, _) = crate::parse("type T = { b: NoSuchType }");
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    // A finalized import: real kind, encoded, owned by another module — the
    // non-overwritable shape whose duplicate check actually fires.
    elab.env.types.insert(
        "T".to_string(),
        TypeDef {
            name: "T".to_string(),
            params: Vec::new(),
            kind: TypeDefKind::Record(vec![("a".to_string(), Type::Nat)]),
            visibility: crate::ast::Visibility::Public,
            span: crate::span::Span::new(0, 0),
            defining_module: Some(crate::elaborate::ModulePath::new(vec!["other".to_string()])),
            encoded_type: Some(Type::Nat),
            field_visibilities: Vec::new(),
        },
    );
    let verdict = elab.run_collection_pass(&ast);
    assert!(verdict.is_err(), "a duplicate type must short-circuit");
    let def = elab.env.types.get("T").expect("the first T must stand");
    assert!(
        matches!(def.encoded_type, Some(Type::Nat)),
        "the healthy finalized definition must not be poisoned: {:?}",
        def.encoded_type
    );
}

/// Application against poison, WITH arguments (ADR 14.8.26g §2.2, as built):
/// the argumentful call is the case a nullary probe false-passes — the
/// zero-argument call never enters the argument loop where the `Arrow`
/// destructure would raise E0013. One seeded signature fault, one call site
/// passing an argument: every reported error is the seeded E0002 (the body
/// pass re-records the signature's fault at the same span, which the display
/// dedup folds) — an E0013 appearing here means the application arm
/// regressed.
#[test]
fn an_argumentful_call_against_poison_stays_quiet() {
    let (ast, parse_errors) =
        crate::parse("fn f(x: Nat) -> NoSuchType { x }\nfn g() -> Nat { f(1) }");
    assert!(parse_errors.is_empty());
    let mut ctx = Context::new();
    let errors = crate::elaborate::elaborate(&ast, &mut ctx)
        .err()
        .expect("the seeded fault must fail the run");
    assert!(
        errors.iter().all(|e| e.kind.code() == "E0002"),
        "the call site must not add its own diagnostic to the seeded E0002: {errors:?}"
    );
}
