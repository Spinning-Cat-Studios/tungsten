//! Term Constructors for FFI
//!
//! This module provides C-compatible functions for constructing Core terms.

use std::ffi::CStr;
use std::os::raw::c_char;

use crate::terms::Term;

use super::{with_arena, TermHandle, TypeHandle, INVALID_HANDLE};

// ============================================================================
// Essential Term Constructors
// ============================================================================

/// Construct a variable term by de Bruijn index.
///
/// The index is converted to a placeholder name like `$0`, `$1`, etc.
/// The actual binding is resolved by the context at type-check time.
#[no_mangle]
pub extern "C" fn tg_term_var(index: u64) -> TermHandle {
    with_arena!(|arena| {
        // Use de Bruijn-style naming: $0, $1, etc.
        let name = format!("${index}");
        arena.alloc_term(Term::Var(name))
    })
}

/// Construct a variable term by name.
///
/// This is useful for referencing named bindings directly.
///
/// # Safety
/// `name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_var_named(name: *const c_char) -> TermHandle {
    if name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };
    with_arena!(|arena| arena.alloc_term(Term::Var(name_str.to_owned())))
}

/// Construct a lambda abstraction: λx:τ. body
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_lambda(
    var_name: *const c_char,
    ty: TypeHandle,
    body: TermHandle,
) -> TermHandle {
    if var_name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(var_name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let body = match arena.get_term(body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Lambda(name_str.to_owned(), ty, Box::new(body)))
    })
}

/// Construct an application: t1 t2
#[no_mangle]
pub extern "C" fn tg_term_app(func: TermHandle, arg: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let func = match arena.get_term(func) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let arg = match arena.get_term(arg) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::App(Box::new(func), Box::new(arg)))
    })
}

/// Construct a let binding: let x : τ = def in body
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_let(
    var_name: *const c_char,
    ty: TypeHandle,
    def: TermHandle,
    body: TermHandle,
) -> TermHandle {
    if var_name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(var_name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let def = match arena.get_term(def) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let body = match arena.get_term(body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Let(
            name_str.to_owned(),
            ty,
            Box::new(def),
            Box::new(body),
        ))
    })
}

/// Construct zero (natural number)
#[no_mangle]
pub extern "C" fn tg_term_zero() -> TermHandle {
    with_arena!(|arena| arena.alloc_term(Term::Zero))
}

/// Construct successor: succ t
#[no_mangle]
pub extern "C" fn tg_term_succ(t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let t = match arena.get_term(t) {
            Some(term) => term.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Succ(Box::new(t)))
    })
}

/// Construct a natural number literal
#[no_mangle]
pub extern "C" fn tg_term_nat_lit(n: u64) -> TermHandle {
    with_arena!(|arena| arena.alloc_term(Term::NatLit(n)))
}

/// Construct boolean true
#[no_mangle]
pub extern "C" fn tg_term_true() -> TermHandle {
    with_arena!(|arena| arena.alloc_term(Term::True))
}

/// Construct boolean false
#[no_mangle]
pub extern "C" fn tg_term_false() -> TermHandle {
    with_arena!(|arena| arena.alloc_term(Term::False))
}

/// Construct unit value: ()
#[no_mangle]
pub extern "C" fn tg_term_unit() -> TermHandle {
    with_arena!(|arena| arena.alloc_term(Term::Unit))
}

/// Construct a string literal
///
/// # Safety
/// `s` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_string(s: *const c_char) -> TermHandle {
    if s.is_null() {
        return INVALID_HANDLE;
    }
    let s_str = match CStr::from_ptr(s).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };
    with_arena!(|arena| arena.alloc_term(Term::StringLit(s_str.to_owned())))
}

// ============================================================================
// Extended Term Constructors
// ============================================================================

/// Construct an if-then-else: if cond then `t_then` else `t_else`
#[no_mangle]
pub extern "C" fn tg_term_if(
    cond: TermHandle,
    t_then: TermHandle,
    t_else: TermHandle,
) -> TermHandle {
    with_arena!(|arena| {
        let cond = match arena.get_term(cond) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t_then = match arena.get_term(t_then) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t_else = match arena.get_term(t_else) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::If(Box::new(cond), Box::new(t_then), Box::new(t_else)))
    })
}

/// Construct a pair: (t1, t2)
#[no_mangle]
pub extern "C" fn tg_term_pair(t1: TermHandle, t2: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let t1 = match arena.get_term(t1) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t2 = match arena.get_term(t2) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Pair(Box::new(t1), Box::new(t2)))
    })
}

/// Construct first projection: fst t
#[no_mangle]
pub extern "C" fn tg_term_fst(t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let t = match arena.get_term(t) {
            Some(term) => term.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Fst(Box::new(t)))
    })
}

/// Construct second projection: snd t
#[no_mangle]
pub extern "C" fn tg_term_snd(t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let t = match arena.get_term(t) {
            Some(term) => term.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Snd(Box::new(t)))
    })
}

/// Construct left injection: inl [τ] t
#[no_mangle]
pub extern "C" fn tg_term_inl(sum_ty: TypeHandle, t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let ty = match arena.get_type(sum_ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t = match arena.get_term(t) {
            Some(term) => term.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Inl(ty, Box::new(t)))
    })
}

/// Construct right injection: inr [τ] t
#[no_mangle]
pub extern "C" fn tg_term_inr(sum_ty: TypeHandle, t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let ty = match arena.get_type(sum_ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t = match arena.get_term(t) {
            Some(term) => term.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Inr(ty, Box::new(t)))
    })
}

/// Construct type annotation: (t : τ)
#[no_mangle]
pub extern "C" fn tg_term_annot(t: TermHandle, ty: TypeHandle) -> TermHandle {
    with_arena!(|arena| {
        let t = match arena.get_term(t) {
            Some(term) => term.clone(),
            None => return INVALID_HANDLE,
        };
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Annot(Box::new(t), ty))
    })
}

/// Construct fix-point: fix f:τ. body
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_fix(
    var_name: *const c_char,
    ty: TypeHandle,
    body: TermHandle,
) -> TermHandle {
    if var_name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(var_name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let body = match arena.get_term(body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Fix(name_str.to_owned(), ty, Box::new(body)))
    })
}

/// Construct global reference
///
/// # Safety
/// `name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_global(name: *const c_char) -> TermHandle {
    if name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };
    with_arena!(|arena| arena.alloc_term(Term::Global(name_str.to_owned())))
}

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

// ============================================================================
// Sum Type and Pattern Matching (Phase 3C-5)
// ============================================================================

/// Construct case analysis on a sum type: case t of inl x => t1 | inr y => t2
///
/// # Safety
/// `left_var` and `right_var` must be valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn tg_term_case(
    scrutinee: TermHandle,
    left_var: *const c_char,
    left_body: TermHandle,
    right_var: *const c_char,
    right_body: TermHandle,
) -> TermHandle {
    if left_var.is_null() || right_var.is_null() {
        return INVALID_HANDLE;
    }
    let left_var_str = match CStr::from_ptr(left_var).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };
    let right_var_str = match CStr::from_ptr(right_var).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        let scrutinee = match arena.get_term(scrutinee) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let left_body = match arena.get_term(left_body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let right_body = match arena.get_term(right_body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Case(
            Box::new(scrutinee),
            left_var_str.to_owned(),
            Box::new(left_body),
            right_var_str.to_owned(),
            Box::new(right_body),
        ))
    })
}

// ============================================================================
// Recursive Types (Phase 3C-5)
// ============================================================================

/// Construct fold: fold [μα.τ] t
///
/// Packs a value into a recursive type.
/// - t : τ[α := μα.τ]
/// - Result: μα.τ
#[no_mangle]
pub extern "C" fn tg_term_fold(mu_ty: TypeHandle, t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let ty = match arena.get_type(mu_ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t = match arena.get_term(t) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Fold(ty, Box::new(t)))
    })
}

/// Construct unfold: unfold [μα.τ] t
///
/// Unpacks a recursive type.
/// - t : μα.τ
/// - Result: τ[α := μα.τ]
#[no_mangle]
pub extern "C" fn tg_term_unfold(mu_ty: TypeHandle, t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let ty = match arena.get_type(mu_ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t = match arena.get_term(t) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Unfold(ty, Box::new(t)))
    })
}

// ============================================================================
// Polymorphism (Phase 3C-5)
// ============================================================================

/// Construct type abstraction: Λα. t
///
/// # Safety
/// `ty_var` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_type_abs(ty_var: *const c_char, body: TermHandle) -> TermHandle {
    if ty_var.is_null() {
        return INVALID_HANDLE;
    }
    let ty_var_str = match CStr::from_ptr(ty_var).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        let body = match arena.get_term(body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::TyAbs(ty_var_str.to_owned(), Box::new(body)))
    })
}

/// Construct type application: t [τ]
#[no_mangle]
pub extern "C" fn tg_term_type_app(t: TermHandle, ty: TypeHandle) -> TermHandle {
    with_arena!(|arena| {
        let t = match arena.get_term(t) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::TyApp(Box::new(t), ty))
    })
}

// ============================================================================
// References (Phase 3C-5)
// ============================================================================

/// Construct a new reference: ref v
#[no_mangle]
pub extern "C" fn tg_term_ref_new(v: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let v = match arena.get_term(v) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::RefNew(Box::new(v)))
    })
}

/// Construct reference dereference: get r
#[no_mangle]
pub extern "C" fn tg_term_ref_get(r: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let r = match arena.get_term(r) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::RefGet(Box::new(r)))
    })
}

/// Construct reference assignment: set r v
#[no_mangle]
pub extern "C" fn tg_term_ref_set(r: TermHandle, v: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let r = match arena.get_term(r) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let v = match arena.get_term(v) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::RefSet(Box::new(r), Box::new(v)))
    })
}

// ============================================================================
// Term Introspection (Phase 3C-5)
// ============================================================================

/// Check if two terms are equal (structurally/α-equivalent)
#[no_mangle]
pub extern "C" fn tg_terms_equal(t1: TermHandle, t2: TermHandle) -> bool {
    with_arena!(|arena| {
        let t1 = match arena.get_term(t1) {
            Some(t) => t,
            None => return false,
        };
        let t2 = match arena.get_term(t2) {
            Some(t) => t,
            None => return false,
        };
        t1 == t2
    })
}

/// Construct a lambda abstraction (alias for `tg_term_lambda`).
///
/// This is a shorter name used by the self-hosted compiler.
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_abs(
    var_name: *const c_char,
    ty: TypeHandle,
    body: TermHandle,
) -> TermHandle {
    tg_term_lambda(var_name, ty, body)
}

/// Construct a sorry term (placeholder for incomplete proofs).
///
/// Sorry terms have a given type but no computational content.
#[no_mangle]
pub extern "C" fn tg_term_sorry(_ty: TypeHandle) -> TermHandle {
    // Note: The type argument is ignored - Term::Sorry doesn't carry a type
    with_arena!(|arena| arena.alloc_term(Term::Sorry))
}
