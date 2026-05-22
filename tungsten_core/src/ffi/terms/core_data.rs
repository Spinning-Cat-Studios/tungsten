//! FFI data constructor functions for primitive and composite term types.
//!
//! Contains: zero, succ, nat_lit, true, false, unit, string, pair, fst, snd, inl, inr.

use std::ffi::CStr;
use std::os::raw::c_char;

use crate::terms::Term;

use crate::ffi::{with_arena, TermHandle, TypeHandle, INVALID_HANDLE};
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
