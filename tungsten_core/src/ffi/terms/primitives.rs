//! Primitive operation term constructors for FFI (arithmetic, boolean, string).
//!
//! These wrap binary and unary operations on Nat, Bool, and String
//! values into Term constructors accessible from C.
//!
//! All constructors are O(1) node pushes (ADR 2.7.26a §4). The repeated
//! binary shape shares one helper.

use super::nodes::TermNode;
use super::{valid_terms, valid_types};
use crate::ffi::{with_arena, TermHandle, TypeHandle, INVALID_HANDLE};
use crate::terms::IntBinOp;

/// Shared O(1) binary-operation constructor.
fn binary_term(
    a: TermHandle,
    b: TermHandle,
    make: fn(TermHandle, TermHandle) -> TermNode,
) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[a, b]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(make(a, b))
    })
}

// ============================================================================
// Arithmetic Term Constructors (Phase 3C)
// ============================================================================

/// Construct natural addition: a + b
#[no_mangle]
pub extern "C" fn tg_term_nat_add(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::NatAdd)
}

/// Construct natural subtraction: a - b (saturating at 0)
#[no_mangle]
pub extern "C" fn tg_term_nat_sub(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::NatSub)
}

/// Construct natural multiplication: a * b
#[no_mangle]
pub extern "C" fn tg_term_nat_mul(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::NatMul)
}

/// Construct natural division: a / b
#[no_mangle]
pub extern "C" fn tg_term_nat_div(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::NatDiv)
}

/// Construct natural modulo: a % b
#[no_mangle]
pub extern "C" fn tg_term_nat_mod(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::NatMod)
}

/// Construct natural equality: a == b
#[no_mangle]
pub extern "C" fn tg_term_nat_eq(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::NatEq)
}

/// Construct natural less-than: a < b
#[no_mangle]
pub extern "C" fn tg_term_nat_lt(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::NatLt)
}

/// Construct natural less-than-or-equal: a <= b
#[no_mangle]
pub extern "C" fn tg_term_nat_le(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::NatLe)
}

/// Construct natural greater-than: a > b
#[no_mangle]
pub extern "C" fn tg_term_nat_gt(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::NatGt)
}

/// Construct natural greater-than-or-equal: a >= b
#[no_mangle]
pub extern "C" fn tg_term_nat_ge(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::NatGe)
}

// ============================================================================
// Boolean Term Constructors (Phase 3C)
// ============================================================================

/// Construct boolean AND: a && b
#[no_mangle]
pub extern "C" fn tg_term_bool_and(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::BoolAnd)
}

/// Construct boolean OR: a || b
#[no_mangle]
pub extern "C" fn tg_term_bool_or(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_term(a, b, TermNode::BoolOr)
}

/// Construct boolean NOT: !a
#[no_mangle]
pub extern "C" fn tg_term_bool_not(a: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[a]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::BoolNot(a))
    })
}

// String term constructors live in the `strings` sibling (ADR 20.8.26c).

/// Construct reflexivity proof: refl [τ] t
///
/// Creates a proof that t equals itself at type τ.
/// The result has type Eq τ t t.
#[no_mangle]
pub extern "C" fn tg_term_refl(ty: TypeHandle, t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        if !valid_types(arena, &[ty]) || !valid_terms(arena, &[t]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Refl(ty, t))
    })
}

/// Construct a substitution proof: subst [τ] [P] eq_proof witness
///
/// Given eq_proof : Eq τ a b and witness : P(a), produces subst : P(b).
#[no_mangle]
pub extern "C" fn tg_term_subst(
    ty: TypeHandle,
    motive: TypeHandle,
    eq_proof: TermHandle,
    witness: TermHandle,
) -> TermHandle {
    with_arena!(|arena| {
        if !valid_types(arena, &[ty, motive]) || !valid_terms(arena, &[eq_proof, witness]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Subst(ty, motive, eq_proof, witness))
    })
}

/// Construct a natural number induction: natind [motive] base step n
#[no_mangle]
pub extern "C" fn tg_term_natind(
    motive: TypeHandle,
    base: TermHandle,
    step: TermHandle,
    n: TermHandle,
) -> TermHandle {
    with_arena!(|arena| {
        if !valid_types(arena, &[motive]) || !valid_terms(arena, &[base, step, n]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::NatInd(motive, base, step, n))
    })
}

// ============================================================================
// Signed Integer Term Constructors (ADR 14.9.26c)
// ============================================================================

/// Construct a signed integer literal.
#[no_mangle]
pub extern "C" fn tg_term_int_lit(value: i64) -> TermHandle {
    with_arena!(|arena| arena.alloc_term_node(TermNode::IntLit(value)))
}

/// Construct a signed binary operation. `op` is [`IntBinOp::code`]
/// (0 = `+`, 1 = `-`, 2 = `*`, 3 = `/`, 4 = `%`, 5 = `==`, 6 = `<`, 7 = `<=`,
/// 8 = `>`, 9 = `>=`); any other code is `INVALID_HANDLE`.
#[no_mangle]
pub extern "C" fn tg_term_int_bin(op: u64, a: TermHandle, b: TermHandle) -> TermHandle {
    let Some(op) = IntBinOp::from_code(op) else {
        return INVALID_HANDLE;
    };
    with_arena!(|arena| {
        if !valid_terms(arena, &[a, b]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::IntBin(op, a, b))
    })
}

/// Construct signed negation: −a
#[no_mangle]
pub extern "C" fn tg_term_int_neg(a: TermHandle) -> TermHandle {
    unary_term(a, TermNode::IntNeg)
}

/// Construct the `Nat → Int` bridge: `to_int(n)`
#[no_mangle]
pub extern "C" fn tg_term_nat_to_int(n: TermHandle) -> TermHandle {
    unary_term(n, TermNode::NatToInt)
}

/// Construct the `Int → Nat` bridge: `from_int(i)`
#[no_mangle]
pub extern "C" fn tg_term_int_to_nat(i: TermHandle) -> TermHandle {
    unary_term(i, TermNode::IntToNat)
}

/// Shared O(1) unary-operation constructor.
fn unary_term(a: TermHandle, make: fn(TermHandle) -> TermNode) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[a]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(make(a))
    })
}

/// Construct a natural number primitive recursion: natrec [ty] base step n
#[no_mangle]
pub extern "C" fn tg_term_natrec(
    ty: TypeHandle,
    base: TermHandle,
    step: TermHandle,
    n: TermHandle,
) -> TermHandle {
    with_arena!(|arena| {
        if !valid_types(arena, &[ty]) || !valid_terms(arena, &[base, step, n]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::NatRec(ty, base, step, n))
    })
}
