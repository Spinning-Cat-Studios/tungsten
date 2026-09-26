//! Exhaustive FFI term-surface tests (ADR 2.7.26a §4 close-out).
//!
//! Companion to `surface_tests.rs` (types): pins every term constructor's
//! produced node via materialize round-trips, the evaluator boundary, and
//! the FFI equality entry points.

use std::ffi::CString;

use super::terms::core::*;
use super::terms::core_data::*;
use super::types::constructors::*;
use super::types::predicates::*;
use super::{tg_init, INVALID_HANDLE};

/// Every term constructor must produce the node it claims: materialize the
/// returned handle and compare with the expected owned `Term`. Kills the
/// `-> Default::default()` constructor mutants (handle 0 is a VALID index,
/// so only asserting on the produced structure distinguishes them).
#[test]
fn term_constructors_produce_their_shapes() {
    use crate::terms::Term;
    use crate::types::Type;

    use super::terms::ext::*;
    use super::terms::nodes::materialize_term;
    use super::terms::primitives::*;
    use super::with_arena_ref;

    tg_init();
    let nat = tg_type_nat();
    let zero = tg_term_zero();
    let one = tg_term_nat_lit(1);
    let tru = tg_term_true();
    let name = CString::new("x").unwrap();

    let mat = |h: super::TermHandle| -> Term {
        with_arena_ref!(|arena| materialize_term(arena, h).expect("valid handle"))
    };

    // (constructed handle, expected owned term) for every constructor shape,
    // split into three builders to keep each within function-size limits.
    let structural = structural_cases(name.as_c_str(), nat, zero, one, tru);
    let primitive = primitive_cases(name.as_c_str(), nat, zero, one, tru);
    let strings = string_cases(zero, one, tru);
    // Non-vacuity: an emptied case builder would make this test pass while
    // checking nothing (a survived `-> vec![]` mutant class).
    assert!(structural.len() >= 25, "structural case table shrank");
    assert!(primitive.len() >= 16, "primitive case table shrank");
    assert!(strings.len() >= 4, "string case table shrank");
    let mut cases = structural;
    cases.extend(primitive);
    cases.extend(strings);
    for (handle, expected) in cases {
        assert_ne!(handle, INVALID_HANDLE);
        assert_eq!(mat(handle), expected);
    }
}

/// Structural/binding constructors: vars, lambda, app, let, if, case, refs…
fn structural_cases(
    name: &std::ffi::CStr,
    nat: super::TypeHandle,
    zero: super::TermHandle,
    one: super::TermHandle,
    tru: super::TermHandle,
) -> Vec<(super::TermHandle, crate::terms::Term)> {
    use super::terms::ext::*;
    use crate::terms::Term;
    use crate::types::Type;
    let bx = Box::new;
    let name = name.as_ptr();
    unsafe {
        vec![
            (zero, Term::Zero),
            (one, Term::NatLit(1)),
            (tru, Term::True),
            (tg_term_false(), Term::False),
            (tg_term_unit(), Term::Unit),
            (tg_term_var(3), Term::Var("$3".into())),
            (tg_term_var_named(name), Term::Var("x".into())),
            (tg_term_global(name), Term::Global("x".into())),
            (tg_term_succ(zero), Term::Succ(bx(Term::Zero))),
            (
                tg_term_lambda(name, nat, zero),
                Term::Lambda("x".into(), Type::Nat, bx(Term::Zero)),
            ),
            (
                tg_term_app(zero, one),
                Term::App(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_let(name, nat, zero, one),
                Term::Let("x".into(), Type::Nat, bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_if(tru, zero, one),
                Term::If(bx(Term::True), bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_annot(zero, nat),
                Term::Annot(bx(Term::Zero), Type::Nat),
            ),
            (
                tg_term_fix(name, nat, zero),
                Term::Fix("x".into(), Type::Nat, bx(Term::Zero)),
            ),
            (
                tg_term_pair(zero, tru),
                Term::Pair(bx(Term::Zero), bx(Term::True)),
            ),
            (tg_term_fst(zero), Term::Fst(bx(Term::Zero))),
            (tg_term_snd(zero), Term::Snd(bx(Term::Zero))),
            (tg_term_string(name), Term::StringLit("x".into())),
            (
                tg_term_case(zero, name, one, name, tru),
                Term::Case(
                    bx(Term::Zero),
                    "x".into(),
                    bx(Term::NatLit(1)),
                    "x".into(),
                    bx(Term::True),
                ),
            ),
            (
                tg_term_fold(nat, zero),
                Term::Fold(Type::Nat, bx(Term::Zero)),
            ),
            (
                tg_term_unfold(nat, zero),
                Term::Unfold(Type::Nat, bx(Term::Zero)),
            ),
            (
                tg_term_type_abs(name, zero),
                Term::TyAbs("x".into(), bx(Term::Zero)),
            ),
            (
                tg_term_type_app(zero, nat),
                Term::TyApp(bx(Term::Zero), Type::Nat),
            ),
            (tg_term_ref_new(zero), Term::RefNew(bx(Term::Zero))),
            (tg_term_ref_get(zero), Term::RefGet(bx(Term::Zero))),
            (
                tg_term_ref_set(zero, one),
                Term::RefSet(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (tg_term_sorry(nat), Term::Sorry),
            (tg_term_return(zero), Term::Return(bx(Term::Zero))),
            (
                tg_term_abs(name, nat, zero),
                Term::Lambda("x".into(), Type::Nat, bx(Term::Zero)),
            ),
        ]
    }
}

/// Primitive-op and proof constructors: arithmetic, bool, string, refl…
fn primitive_cases(
    name: &std::ffi::CStr,
    nat: super::TypeHandle,
    zero: super::TermHandle,
    one: super::TermHandle,
    tru: super::TermHandle,
) -> Vec<(super::TermHandle, crate::terms::Term)> {
    use super::terms::ext::*;
    use super::terms::primitives::*;
    use crate::terms::Term;
    use crate::types::Type;
    let bx = Box::new;
    let name = name.as_ptr();
    unsafe {
        vec![
            (
                tg_term_nat_add(zero, one),
                Term::NatAdd(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_nat_sub(zero, one),
                Term::NatSub(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_nat_mul(zero, one),
                Term::NatMul(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_nat_div(zero, one),
                Term::NatDiv(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_nat_mod(zero, one),
                Term::NatMod(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_nat_eq(zero, one),
                Term::NatEq(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_nat_lt(zero, one),
                Term::NatLt(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_nat_le(zero, one),
                Term::NatLe(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_nat_gt(zero, one),
                Term::NatGt(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_nat_ge(zero, one),
                Term::NatGe(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_bool_and(tru, tru),
                Term::BoolAnd(bx(Term::True), bx(Term::True)),
            ),
            (
                tg_term_bool_or(tru, tru),
                Term::BoolOr(bx(Term::True), bx(Term::True)),
            ),
            (tg_term_bool_not(tru), Term::BoolNot(bx(Term::True))),
            (
                tg_term_refl(nat, zero),
                Term::Refl(Type::Nat, bx(Term::Zero)),
            ),
            (
                tg_term_subst(nat, nat, zero, one),
                Term::Subst(Type::Nat, Type::Nat, bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_natind(nat, zero, one, tru),
                Term::NatInd(
                    Type::Nat,
                    bx(Term::Zero),
                    bx(Term::NatLit(1)),
                    bx(Term::True),
                ),
            ),
            (
                tg_term_natrec(nat, zero, one, tru),
                Term::NatRec(
                    Type::Nat,
                    bx(Term::Zero),
                    bx(Term::NatLit(1)),
                    bx(Term::True),
                ),
            ),
            (tg_term_inl(nat, zero), Term::Inl(Type::Nat, bx(Term::Zero))),
            (tg_term_inr(nat, zero), Term::Inr(Type::Nat, bx(Term::Zero))),
        ]
    }
}

// The string constructors' cases live beside them, in
// `terms::strings::tests` — they moved out of `primitives` with the functions
// (ADR 20.8.26c). `string_cases` here is their re-export, so this file's
// "every constructor shape" claim stays literally true.
use super::terms::strings::tests::string_cases;

/// FFI equality entry points must distinguish both verdicts.
#[test]
fn ffi_equality_entry_points_report_both_verdicts() {
    use super::check::tg_types_equal;
    use super::terms::ext::tg_terms_equal;

    tg_init();
    let nat_a = tg_type_nat();
    let nat_b = tg_type_nat();
    let bool_ty = tg_type_bool();
    assert!(tg_types_equal(nat_a, nat_b));
    assert!(!tg_types_equal(nat_a, bool_ty));
    assert!(!tg_types_equal(nat_a, INVALID_HANDLE));

    let zero_a = tg_term_zero();
    let zero_b = tg_term_zero();
    let tru = tg_term_true();
    assert!(tg_terms_equal(zero_a, zero_b));
    assert!(!tg_terms_equal(zero_a, tru));
    assert!(!tg_terms_equal(zero_a, INVALID_HANDLE));
}

/// `tg_type_is_app` has no FFI constructor for its true path — App nodes
/// enter the arena only via kernel-result import. Import one directly.
#[test]
fn is_app_holds_on_imported_app_nodes() {
    use super::types::nodes::import_type;
    use super::with_arena;
    use crate::types::Type;

    tg_init();
    let app = with_arena!(|arena| import_type(arena, &Type::app("Forest", vec![Type::Nat])));
    assert!(tg_type_is_app(app));
    assert!(!tg_type_is_mu(app));
}

/// The evaluator entry must materialize, evaluate, and re-import through
/// the node arena: NatAdd(1, 1) evaluates to 2.
#[test]
fn eval_run_round_trips_through_the_node_arena() {
    use super::driver::{tg_eval_env_free, tg_eval_env_new, tg_eval_run_with_limit};
    use super::terms::nodes::materialize_term;
    use super::terms::primitives::tg_term_nat_add;
    use super::with_arena_ref;
    use crate::terms::Term;

    tg_init();
    let one_a = tg_term_nat_lit(1);
    let one_b = tg_term_nat_lit(1);
    let sum = tg_term_nat_add(one_a, one_b);
    let env = tg_eval_env_new();
    let result = tg_eval_run_with_limit(sum, env, 1000);
    assert_ne!(result, INVALID_HANDLE);
    let value =
        with_arena_ref!(|arena| materialize_term(arena, result).expect("valid result handle"));
    // Small numbers evaluate to canonical Succ form (proof compatibility).
    assert_eq!(
        value,
        Term::Succ(Box::new(Term::Succ(Box::new(Term::Zero))))
    );
    assert_eq!(
        tg_eval_run_with_limit(INVALID_HANDLE, env, 1000),
        INVALID_HANDLE
    );
    tg_eval_env_free(env);
}

/// `limit == 0` is the unbounded-evaluation sentinel, not a zero-step budget
/// (kills the `==` → `!=` branch-selector mutant, ADR 22.7.26a close-out):
/// limit 0 must evaluate to completion, while a too-small nonzero limit must
/// exhaust and report `INVALID_HANDLE` through the driver error.
#[test]
fn eval_run_limit_zero_is_unbounded_and_small_limits_exhaust() {
    use super::driver::{tg_eval_env_free, tg_eval_env_new, tg_eval_run_with_limit};
    use super::terms::primitives::tg_term_nat_add;

    tg_init();
    let env = tg_eval_env_new();

    let unbounded_sum = tg_term_nat_add(tg_term_nat_lit(1), tg_term_nat_lit(1));
    assert_ne!(
        tg_eval_run_with_limit(unbounded_sum, env, 0),
        INVALID_HANDLE,
        "limit 0 selects the unbounded entry and must reach the value"
    );

    let exhausted_sum = tg_term_nat_add(tg_term_nat_lit(1), tg_term_nat_lit(1));
    assert_eq!(
        tg_eval_run_with_limit(exhausted_sum, env, 1),
        INVALID_HANDLE,
        "a 1-step limit cannot finish NatAdd and must report the step-limit error"
    );
    tg_eval_env_free(env);
}

/// `tg_eval_display_value` must render through the node arena (kills the
/// null-return mutant).
#[test]
fn eval_display_value_renders_materialized_terms() {
    use super::driver::{tg_eval_display, tg_eval_display_value};

    tg_init();
    let zero = tg_term_zero();
    // Debug-format sibling: also materializes through the node arena.
    let dbg_ptr = tg_eval_display(zero);
    assert!(!dbg_ptr.is_null());
    let dbg = unsafe { std::ffi::CStr::from_ptr(dbg_ptr) }
        .to_str()
        .unwrap()
        .to_owned();
    unsafe { drop(CString::from_raw(dbg_ptr)) };
    assert_eq!(dbg, "Zero");
    assert!(tg_eval_display(INVALID_HANDLE).is_null());

    let ptr = tg_eval_display_value(zero);
    assert!(!ptr.is_null());
    let rendered = unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_str()
        .unwrap()
        .to_owned();
    unsafe { drop(CString::from_raw(ptr)) };
    assert_eq!(rendered, "0");
    assert!(tg_eval_display_value(INVALID_HANDLE).is_null());
}
