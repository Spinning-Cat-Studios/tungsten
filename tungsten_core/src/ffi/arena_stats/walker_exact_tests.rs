//! Exact-value arithmetic tests for the retention walkers (mutation-gate hardening).
//!
//! The `walkers_agree…` inequality test (in `tests.rs`) catches missed
//! children but not `+`/`-`/`*` slips; these pin the arithmetic of every
//! distinct walker arm shape exactly (ADR 2.7.26a / 11.7.26a).

use super::*;
use crate::ffi::test_support::sized_name;

// ---- Exact-value arithmetic tests (mutation-gate hardening) ----------
//
// The `walkers_agree…` inequality test below catches missed children but
// not `+`/`-`/`*` slips; these pin the arithmetic of every distinct
// `deep_term_bytes` arm shape exactly. (`deep_type_bytes`'s arms are
// exercised transitively via embedded types with known sizes.)

const TERM: u64 = size_of::<Term>() as u64;
const TYPE: u64 = size_of::<Type>() as u64;

/// A type with a known non-zero deep size: Arrow(TyVar("abcd"), Nat)
/// = 2 boxed children + 4 bytes of string capacity.
fn known_type() -> (Type, u64) {
    let ty = Type::Arrow(
        Box::new(Type::TyVar(sized_name("abcd"))),
        Box::new(Type::Nat),
    );
    (ty, 2 * TYPE + 4)
}

#[test]
fn type_arms_are_exact() {
    // Product/Sum mirror Arrow: 2 boxed children + child heap.
    let t = Type::Product(Box::new(Type::TyVar(sized_name("ab"))), Box::new(Type::Nat));
    assert_eq!(deep_type_bytes(&t), 2 * TYPE + 2);
    let t = Type::Sum(
        Box::new(Type::Nat),
        Box::new(Type::TyVar(sized_name("xyz"))),
    );
    assert_eq!(deep_type_bytes(&t), 2 * TYPE + 3);
    // Forall/Mu: binder cap + one boxed child (+ child heap).
    let (inner, inner_bytes) = known_type();
    let t = Type::Forall(sized_name("alpha"), Box::new(inner));
    assert_eq!(deep_type_bytes(&t), 5 + TYPE + inner_bytes);
    let (inner, inner_bytes) = known_type();
    let t = Type::Mu(sized_name("a_List"), Box::new(inner));
    assert_eq!(deep_type_bytes(&t), 6 + TYPE + inner_bytes);
    // Ptr/Ref: one boxed child + child heap.
    let (inner, inner_bytes) = known_type();
    assert_eq!(
        deep_type_bytes(&Type::Ptr(Box::new(inner))),
        TYPE + inner_bytes
    );
    let (inner, inner_bytes) = known_type();
    assert_eq!(
        deep_type_bytes(&Type::Ref(Box::new(inner))),
        TYPE + inner_bytes
    );
}

#[test]
fn eq_app_and_adt_type_arms_are_exact() {
    // Eq(t, lhs, rhs): boxed type + child heap + 2 boxed terms + term heap.
    let (inner, inner_bytes) = known_type();
    let t = Type::Eq(
        Box::new(inner),
        Box::new(Term::Var(sized_name("lhs"))),
        Box::new(Term::Var(sized_name("rh"))),
    );
    assert_eq!(deep_type_bytes(&t), TYPE + inner_bytes + 2 * TERM + 3 + 2);
    // App(name, args): name cap + capacity*TYPE slab + Σ arg heap.
    let args = vec![Type::TyVar(sized_name("ab")), Type::Nat];
    let cap = args.capacity() as u64;
    let t = Type::App(sized_name("Forest"), args);
    assert_eq!(deep_type_bytes(&t), 6 + cap * TYPE + 2);
    // Adt(name, type_args, variants): name cap + both vec slabs +
    // Σ type-arg heap + Σ (ctor-name cap + payload heap). The payload
    // must own non-zero heap or the `ctor-cap + payload` add is
    // mutation-invisible (x + 0 == x - 0).
    let type_args = vec![Type::TyVar(sized_name("t"))];
    let ta_cap = type_args.capacity() as u64;
    let variants = vec![(sized_name("Leaf"), Type::TyVar(sized_name("pay")))];
    let v_cap = variants.capacity() as u64;
    let v_slot = size_of::<(String, Type)>() as u64;
    let t = Type::Adt(sized_name("Tree"), type_args, variants);
    assert_eq!(
        deep_type_bytes(&t),
        4 + ta_cap * TYPE + 1 + v_cap * v_slot + 4 + 3
    );
}

#[test]
fn term_arms_with_var_type_and_body_are_exact() {
    let (ty, ty_bytes) = known_type();
    // Lambda(v, ty, body): v.cap + deep(ty) + (TERM + deep(body))
    let t = Term::Lambda(sized_name("xy"), ty, Box::new(Term::Var(sized_name("abc"))));
    assert_eq!(deep_term_bytes(&t), 2 + ty_bytes + TERM + 3);
    // Let(v, ty, a, b): v.cap + deep(ty) + 2 boxed children
    let (ty2, ty2_bytes) = known_type();
    let t = Term::Let(
        sized_name("v"),
        ty2,
        Box::new(Term::Zero),
        Box::new(Term::Var(sized_name("ab"))),
    );
    assert_eq!(deep_term_bytes(&t), 1 + ty2_bytes + (TERM) + (TERM + 2));
}

#[test]
fn binary_ternary_and_case_arms_are_exact() {
    // App(a, b) with string-bearing children: 2*TERM + child caps
    let t = Term::App(
        Box::new(Term::Var(sized_name("abc"))),
        Box::new(Term::Global(sized_name("de"))),
    );
    assert_eq!(deep_term_bytes(&t), 2 * TERM + 3 + 2);
    // If(c, t, e): 3 boxed children
    let t = Term::If(
        Box::new(Term::Var(sized_name("a"))),
        Box::new(Term::Zero),
        Box::new(Term::Zero),
    );
    assert_eq!(deep_term_bytes(&t), 3 * TERM + 1);
    // Case(s, v1, a, v2, b): both binder caps + 3 boxed children
    let t = Term::Case(
        Box::new(Term::Zero),
        sized_name("left"),
        Box::new(Term::Var(sized_name("l"))),
        sized_name("right"),
        Box::new(Term::Var(sized_name("r"))),
    );
    assert_eq!(deep_term_bytes(&t), 4 + 5 + 3 * TERM + 1 + 1);
}

#[test]
fn type_carrying_unary_and_quaternary_arms_are_exact() {
    let (ty, ty_bytes) = known_type();
    // Fold(ty, t): deep(ty) + boxed child
    let t = Term::Fold(ty, Box::new(Term::Var(sized_name("abc"))));
    assert_eq!(deep_term_bytes(&t), ty_bytes + TERM + 3);
    // NatRec(ty, a, b, c): deep(ty) + 3 boxed children
    let (ty2, ty2_bytes) = known_type();
    let t = Term::NatRec(
        ty2,
        Box::new(Term::Zero),
        Box::new(Term::Var(sized_name("a"))),
        Box::new(Term::Zero),
    );
    assert_eq!(deep_term_bytes(&t), ty2_bytes + 3 * TERM + 1);
    // Subst(ty1, ty2, a, b): both type deeps + 2 boxed children
    let (ty3, ty3_bytes) = known_type();
    let (ty4, ty4_bytes) = known_type();
    let t = Term::Subst(ty3, ty4, Box::new(Term::Zero), Box::new(Term::Zero));
    assert_eq!(deep_term_bytes(&t), ty3_bytes + ty4_bytes + 2 * TERM);
    // TyApp(t, ty) / TyAbs(v, t)
    let (ty5, ty5_bytes) = known_type();
    let t = Term::TyApp(Box::new(Term::Var(sized_name("f"))), ty5);
    assert_eq!(deep_term_bytes(&t), ty5_bytes + TERM + 1);
    let t = Term::TyAbs(sized_name("alpha"), Box::new(Term::Zero));
    assert_eq!(deep_term_bytes(&t), 5 + TERM);
}

#[test]
fn vec_carrying_arms_are_exact() {
    // ExternCall(name, args): name.cap + capacity*TERM + Σ deep(elem)
    let args = vec![Term::Var(sized_name("ab")), Term::Zero];
    let cap = args.capacity() as u64;
    let t = Term::ExternCall(sized_name("tg_fn"), args);
    assert_eq!(deep_term_bytes(&t), 5 + cap * TERM + 2);
    // AdtMatch(s, arms): boxed scrutinee + capacity*slot + Σ(v.cap + boxed body)
    let arms = vec![(
        0usize,
        sized_name("ctor"),
        Box::new(Term::Var(sized_name("x"))),
    )];
    let cap = arms.capacity() as u64;
    let slot = size_of::<(usize, String, Box<Term>)>() as u64;
    let t = Term::AdtMatch(Box::new(Term::Zero), arms);
    assert_eq!(deep_term_bytes(&t), TERM + cap * slot + 4 + (TERM + 1));
    // AdtConstruct(ty, idx, t)
    let (ty, ty_bytes) = known_type();
    let t = Term::AdtConstruct(ty, 3, Box::new(Term::Var(sized_name("p"))));
    assert_eq!(deep_term_bytes(&t), ty_bytes + TERM + 1);
}

#[test]
fn spanned_and_string_arms_are_exact() {
    let t = Term::Spanned(
        Box::new(Term::Var(sized_name("abc"))),
        crate::terms::TermSpan::default(),
    );
    assert_eq!(deep_term_bytes(&t), TERM + 3);
    assert_eq!(deep_term_bytes(&Term::StringLit(sized_name("hello"))), 5);
}

#[test]
fn ctx_bytes_are_exact() {
    let (ty, ty_bytes) = known_type();
    let ctx = Context::new()
        .with_term(sized_name("xvar"), ty)
        .with_type_var(sized_name("beta"));
    let cap = ctx.bindings().capacity() as u64;
    let slot = size_of::<Binding>() as u64;
    // Σ per binding: Term("xvar", ty) → 4 + ty_bytes; TypeVar("beta") → 4.
    assert_eq!(deep_ctx_bytes(&ctx), cap * slot + 4 + ty_bytes + 4);
}

#[test]
fn marker_line_arithmetic_is_exact() {
    let mut arena = Arena::new();
    // Deterministic state via direct field writes: exact deep counters,
    // known entry counts, and MULTI-MB slabs with distinct per-class
    // contributions — sub-MB slabs would let arithmetic mutations
    // quantize away in the `/ (1024*1024)` rendering.
    arena.types.push(crate::ffi::types::nodes::TypeNode::Nat);
    arena.terms.push(crate::ffi::terms::nodes::TermNode::Zero);
    arena.types.reserve(200_000);
    arena.terms.reserve(100_000);
    arena.ctxs.reserve(50_000);
    arena.retention.types = 5 * 1024 * 1024;
    arena.retention.terms = 3 * 1024 * 1024;
    let type_node_size = size_of::<crate::ffi::types::nodes::TypeNode>() as u64;
    let term_node_size = size_of::<crate::ffi::terms::nodes::TermNode>() as u64;
    let slab_mb = ((arena.types.capacity() as u64) * type_node_size
        + (arena.terms.capacity() as u64) * term_node_size
        + (arena.ctxs.capacity() as u64) * size_of::<Context>() as u64)
        / (1024 * 1024);
    assert!(
        slab_mb >= 10,
        "slabs must span many MB for mutant visibility"
    );
    let line = marker_line_with_rss(&arena, Some(2 * 1024 * 1024)); // 2 GiB in KiB
    assert_eq!(
        line,
        format!("  [arena] types=1 deep=5MB terms=1 deep=3MB ctxs=0 slab={slab_mb}MB vmrss=2048MB")
    );
    // The na path (off-Linux) renders verbatim.
    let line = marker_line_with_rss(&arena, None);
    assert!(line.ends_with("vmrss=naMB"), "{line}");
}
