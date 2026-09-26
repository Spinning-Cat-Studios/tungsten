//! Tests: that the inner-μ peel *stops* (ADR 11.8.26c).
//!
//! Its sibling `tests.rs` covers the complementary question — what the peel
//! unfolds *to*. The seam matters here because the two failure modes are
//! independent: a peel can be correct on every input it returns from and still
//! never return, which is exactly what this module exists to pin.
//!
//! ## Why a wall-clock harness rather than a plain assertion
//!
//! Pre-fix, `unfold_inner_mu_layers` on the `Rose` encoding does not return a
//! wrong answer — it returns no answer. A `#[test]` calling it directly does
//! not go red, it hangs `cargo test`, which is uncommittable. So the call runs
//! on a worker thread under `recv_timeout` and the pre-fix failure shape is
//! *"exceeded the bound"*, following `bootstrap/tests/eval_generic_mu_budget.rs`.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tungsten_core::types::unfold_mu_type;
use tungsten_core::Type;

use super::test_fixtures::{
    ctor, elaborator_with_asymmetric_mutual_pair, elaborator_with_rose, make_elaborator, type_def,
};
use super::{ResidualMuChain, UnflattenedMu, UnflattenedMuCause};
use crate::elaborate::env::TypeDefKind;
use crate::elaborate::Elaborator;

/// Generous ceiling: the guarded peel finishes in microseconds, and the
/// pre-guard loop ran unbounded (killed at 25 s and at 120 s on the CLI
/// reproducer). Anything near this bound is broken, not slow.
const PEEL_BUDGET: Duration = Duration::from_secs(10);

/// Run a peel on a worker thread and fail the test if it does not return
/// inside [`PEEL_BUDGET`].
///
/// The elaborator is built *inside* the worker: `Elaborator` is not `Send`,
/// and more to the point, a builder closure keeps the pre-fix hang confined to
/// the worker rather than the harness.
fn peel_within_budget(
    build: fn() -> Elaborator<'static>,
    input: Type,
    what: &str,
) -> Result<Type, UnflattenedMu> {
    let (sender, receiver) = mpsc::channel();
    // Default stack, deliberately. `eval_generic_mu_budget.rs` raises its
    // worker's stack because it evaluates a depth-1003 Peano chain; this peel
    // is a loop over a two-node type and recurses only as deep as `substitute`
    // does. Copying the `stack_size` from that precedent would be cargo cult —
    // and it showed up as an unkillable mutation site, since no assertion can
    // distinguish one stack size from another here.
    thread::Builder::new()
        .spawn(move || {
            let elab = build();
            // The receiver may have given up already — ignore send errors.
            let _ = sender
                .send(elab.unfold_inner_mu_layers(ResidualMuChain::after_outer_unfold(input)));
        })
        .expect("failed to spawn peel worker");

    receiver.recv_timeout(PEEL_BUDGET).unwrap_or_else(|_| {
        panic!(
            "unfold_inner_mu_layers on {what} exceeded the {PEEL_BUDGET:?} budget — \
             the ADR 11.8.26c fixpoint guard is not stopping the peel"
        )
    })
}

// ============================================================================
// AC 1 — the nested family returns, inside a bound
// ============================================================================

/// The whole bug, pinned: `μα_Rose. α_Rose` substitutes to itself, so every
/// iteration re-enters on an identical type. Pre-guard this ran forever at
/// flat RSS; post-guard it stops on the second sighting of `α_Rose`.
#[test]
fn nested_family_peel_returns_inside_the_budget() {
    let rose_encoding = Type::mu("α_Rose", Type::TyVar("α_Rose".to_string()));
    let outcome = peel_within_budget(elaborator_with_rose, rose_encoding, "the `Rose` encoding");

    let err = outcome.expect_err("a vacuous μ cannot flatten to a structural head");
    assert_eq!(
        err,
        UnflattenedMu {
            binder: "α_Rose".to_string(),
            cause: UnflattenedMuCause::BinderRepeats,
        },
        "the residual must name the binder that would not flatten"
    );
    assert_eq!(err.type_name(), "Rose", "α_ prefix should be stripped");
}

// ============================================================================
// AC 4 — non-vacuity: legitimate encodings still flatten
// ============================================================================

/// Non-vacuity for AC 1: without this, "reject everything μ-headed" would
/// satisfy the nested-family tests. An ordinary self-recursive ADT resolves
/// its single binder and reaches a `Sum` head.
#[test]
fn ordinary_recursive_adt_still_flattens() {
    let mut elab = make_elaborator();
    let list_encoding = Type::mu(
        "α_List",
        Type::sum(Type::Unit, Type::TyVar("α_List".to_string())),
    );
    elab.env.define_type(type_def(
        "List",
        TypeDefKind::ADT(vec![
            ctor("Nil", 0, vec![]),
            ctor("Cons", 1, vec![Type::TyVar("List".to_string())]),
        ]),
        Some(list_encoding.clone()),
    ));

    let result = elab
        .unfold_inner_mu_layers(ResidualMuChain::after_outer_unfold(list_encoding))
        .expect("an ordinary recursive ADT flattens");
    assert!(
        matches!(result, Type::Sum(_, _)),
        "expected a Sum head, got {result:?}"
    );
}

/// A **mutual** pair still flattens, and the binder chain is longer than one —
/// so the visited-binder list must admit distinct binders rather than stopping
/// at the first μ it sees.
#[test]
fn mutual_pair_still_flattens() {
    let elab = elaborator_with_asymmetric_mutual_pair();

    // What `unfold_scrutinee_type` hands over after peeling A's outer binder.
    let residual = Type::mu("α_B", Type::sum(Type::Unit, Type::TyVar("α_B".to_string())));
    let result = elab
        .unfold_inner_mu_layers(ResidualMuChain::after_outer_unfold(residual))
        .expect("a mutual pair flattens");

    let Type::Sum(_, right) = &result else {
        panic!("expected a Sum head, got {result:?}");
    };
    // `AB`'s field is `B`, so the right summand must be *B's* stored encoding.
    assert_eq!(
        **right,
        Type::mu(
            "α_A",
            Type::mu(
                "α_B",
                Type::product(Type::TyVar("α_A".to_string()), Type::Nat),
            ),
        ),
        "α_B must resolve to B's own cached encoding, not to the input"
    );
}

// ============================================================================
// P1 — why `unfold_mu_type` is not a drop-in (ADR 11.8.26c §2.1)
// ============================================================================

/// The ADR's P1 question, settled as a test rather than as an argument.
///
/// `unfold_mu_type` replaces every chain variable with the whole *input*
/// μ-type. On the residual chain this loop receives — after the outer binder
/// has already been peeled — the input no longer denotes the group, so `α_B`
/// resolves to an A-shaped type (`Unit + itself`) instead of to `B`
/// (`A × Nat`). The two answers differ, and the env-keyed one is the correct
/// one, so the canonical unfolder cannot simply replace this loop.
///
/// If a future ADR makes them agree, this test goes red — which is the
/// notification that deleting `unfold_inner_mu_layers` has become possible.
#[test]
fn unfold_mu_layers_disagrees_with_canonical_on_mutual_pair() {
    let elab = elaborator_with_asymmetric_mutual_pair();
    let residual = Type::mu("α_B", Type::sum(Type::Unit, Type::TyVar("α_B".to_string())));

    let env_keyed = elab
        .unfold_inner_mu_layers(ResidualMuChain::after_outer_unfold(residual.clone()))
        .expect("a mutual pair flattens");
    let canonical = unfold_mu_type(&residual);

    assert_ne!(
        env_keyed, canonical,
        "if these agree, unfold_mu_type is a drop-in and this loop should be deleted \
         (ADR 11.8.26c §2.1 P1)"
    );

    // And the disagreement is not incidental: the canonical answer puts an
    // A-shaped type where `AB`'s field type is `B`.
    let Type::Sum(_, canonical_right) = &canonical else {
        panic!("expected a Sum head, got {canonical:?}");
    };
    assert_eq!(
        **canonical_right, residual,
        "canonical substitutes the input"
    );
}

// ============================================================================
// AC 5 — the env-miss fallback is gone
// ============================================================================

/// The removed `unwrap_or_else(|| ty.clone())` made the step
/// `ty = body.substitute(var, &ty)` — the current type substituted into its
/// own body, which is precisely the accumulating shape ADR 7.7.26k banned,
/// sitting inside the allowlist entry whose justification read "*not* the
/// accumulated type". A binder with no cached encoding now rejects instead.
#[test]
fn missing_env_encoding_rejects_rather_than_self_substituting() {
    let elab = make_elaborator(); // no types defined at all
    let orphan = Type::mu(
        "α_Ghost",
        Type::sum(Type::Unit, Type::TyVar("α_Ghost".to_string())),
    );

    let err = elab
        .unfold_inner_mu_layers(ResidualMuChain::after_outer_unfold(orphan))
        .expect_err("a binder with no cached encoding cannot be resolved");
    assert_eq!(
        err,
        UnflattenedMu {
            binder: "α_Ghost".to_string(),
            cause: UnflattenedMuCause::MissingEncoding,
        }
    );
}

/// A type registered *without* a cached encoding (a parameterized type, or a
/// stub from Type-Name Registration) takes the same path — the lookup has to
/// yield an encoding, not merely a `TypeDef`.
#[test]
fn env_entry_without_an_encoding_also_rejects() {
    let mut elab = make_elaborator();
    elab.env
        .define_type(type_def("Ghost", TypeDefKind::Stub, None));

    let err = elab
        .unfold_inner_mu_layers(ResidualMuChain::after_outer_unfold(Type::mu(
            "α_Ghost",
            Type::TyVar("α_Ghost".to_string()),
        )))
        .expect_err("a TypeDef with no encoded_type cannot resolve its binder");
    assert_eq!(err.cause, UnflattenedMuCause::MissingEncoding);
}
