//! Primitive operation term constructors for FFI (arithmetic, boolean, string).
//!
//! These wrap binary and unary operations on Nat, Bool, and String
//! values into Term constructors accessible from C.

use crate::terms::Term;

use crate::ffi::{with_arena, TermHandle, TypeHandle, INVALID_HANDLE};
// ============================================================================
// Arithmetic Term Constructors (Phase 3C)
// ============================================================================

/// Construct natural addition: a + b
#[no_mangle]
pub extern "C" fn tg_term_nat_add(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::NatAdd(Box::new(a), Box::new(b)))
    })
}

/// Construct natural subtraction: a - b (saturating at 0)
#[no_mangle]
pub extern "C" fn tg_term_nat_sub(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::NatSub(Box::new(a), Box::new(b)))
    })
}

/// Construct natural multiplication: a * b
#[no_mangle]
pub extern "C" fn tg_term_nat_mul(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::NatMul(Box::new(a), Box::new(b)))
    })
}

/// Construct natural division: a / b
#[no_mangle]
pub extern "C" fn tg_term_nat_div(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::NatDiv(Box::new(a), Box::new(b)))
    })
}

/// Construct natural modulo: a % b
#[no_mangle]
pub extern "C" fn tg_term_nat_mod(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::NatMod(Box::new(a), Box::new(b)))
    })
}

/// Construct natural equality: a == b
#[no_mangle]
pub extern "C" fn tg_term_nat_eq(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::NatEq(Box::new(a), Box::new(b)))
    })
}

/// Construct natural less-than: a < b
#[no_mangle]
pub extern "C" fn tg_term_nat_lt(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::NatLt(Box::new(a), Box::new(b)))
    })
}

/// Construct natural less-than-or-equal: a <= b
#[no_mangle]
pub extern "C" fn tg_term_nat_le(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::NatLe(Box::new(a), Box::new(b)))
    })
}

/// Construct natural greater-than: a > b
#[no_mangle]
pub extern "C" fn tg_term_nat_gt(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::NatGt(Box::new(a), Box::new(b)))
    })
}

/// Construct natural greater-than-or-equal: a >= b
#[no_mangle]
pub extern "C" fn tg_term_nat_ge(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::NatGe(Box::new(a), Box::new(b)))
    })
}

// ============================================================================
// Boolean Term Constructors (Phase 3C)
// ============================================================================

/// Construct boolean AND: a && b
#[no_mangle]
pub extern "C" fn tg_term_bool_and(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::BoolAnd(Box::new(a), Box::new(b)))
    })
}

/// Construct boolean OR: a || b
#[no_mangle]
pub extern "C" fn tg_term_bool_or(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::BoolOr(Box::new(a), Box::new(b)))
    })
}

/// Construct boolean NOT: !a
#[no_mangle]
pub extern "C" fn tg_term_bool_not(a: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::BoolNot(Box::new(a)))
    })
}

// ============================================================================
// String Term Constructors (Phase 3C)
// ============================================================================

/// Construct string concatenation: a ++ b
#[no_mangle]
pub extern "C" fn tg_term_str_concat(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::StrConcat(Box::new(a), Box::new(b)))
    })
}

/// Construct string equality: a == b
#[no_mangle]
pub extern "C" fn tg_term_str_eq(a: TermHandle, b: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let a = match arena.get_term(a) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let b = match arena.get_term(b) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::StrEq(Box::new(a), Box::new(b)))
    })
}

/// Construct string length: strlen s
#[no_mangle]
pub extern "C" fn tg_term_str_len(s: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let s = match arena.get_term(s) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::StrLen(Box::new(s)))
    })
}

/// Construct reflexivity proof: refl [τ] t
///
/// Creates a proof that t equals itself at type τ.
/// The result has type Eq τ t t.
#[no_mangle]
pub extern "C" fn tg_term_refl(ty: TypeHandle, t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t = match arena.get_term(t) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::refl(ty, t))
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
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let motive = match arena.get_type(motive) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let eq = match arena.get_term(eq_proof) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let witness = match arena.get_term(witness) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::subst(ty, motive, eq, witness))
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
        let motive = match arena.get_type(motive) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let base = match arena.get_term(base) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let step = match arena.get_term(step) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let n = match arena.get_term(n) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::natind(motive, base, step, n))
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
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let base = match arena.get_term(base) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let step = match arena.get_term(step) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let n = match arena.get_term(n) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::natrec(ty, base, step, n))
    })
}
