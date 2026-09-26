//! FFI data constructor functions for primitive and composite term types.
//!
//! Contains: zero, succ, nat_lit, true, false, unit, string, pair, fst, snd, inl, inr.
//!
//! All constructors are O(1) node pushes (ADR 2.7.26a §4).

use std::ffi::CStr;
use std::os::raw::c_char;

use super::nodes::TermNode;
use super::{valid_terms, valid_types};
use crate::ffi::{with_arena, TermHandle, TypeHandle, INVALID_HANDLE};

/// Construct zero (natural number)
#[no_mangle]
pub extern "C" fn tg_term_zero() -> TermHandle {
    with_arena!(|arena| arena.alloc_term_node(TermNode::Zero))
}

/// Construct successor: succ t
#[no_mangle]
pub extern "C" fn tg_term_succ(t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[t]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Succ(t))
    })
}

/// Construct a natural number literal
#[no_mangle]
pub extern "C" fn tg_term_nat_lit(n: u64) -> TermHandle {
    with_arena!(|arena| arena.alloc_term_node(TermNode::NatLit(n)))
}

/// Construct boolean true
#[no_mangle]
pub extern "C" fn tg_term_true() -> TermHandle {
    with_arena!(|arena| arena.alloc_term_node(TermNode::True))
}

/// Construct boolean false
#[no_mangle]
pub extern "C" fn tg_term_false() -> TermHandle {
    with_arena!(|arena| arena.alloc_term_node(TermNode::False))
}

/// Construct unit value: ()
#[no_mangle]
pub extern "C" fn tg_term_unit() -> TermHandle {
    with_arena!(|arena| arena.alloc_term_node(TermNode::Unit))
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
    with_arena!(|arena| arena.alloc_term_node(TermNode::StringLit(s_str.to_owned())))
}

/// Construct a pair: (t1, t2)
#[no_mangle]
pub extern "C" fn tg_term_pair(t1: TermHandle, t2: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[t1, t2]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Pair(t1, t2))
    })
}

/// Construct first projection: fst t
#[no_mangle]
pub extern "C" fn tg_term_fst(t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[t]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Fst(t))
    })
}

/// Construct second projection: snd t
#[no_mangle]
pub extern "C" fn tg_term_snd(t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[t]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Snd(t))
    })
}

/// Construct left injection: inl [τ] t
#[no_mangle]
pub extern "C" fn tg_term_inl(sum_ty: TypeHandle, t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        if !valid_types(arena, &[sum_ty]) || !valid_terms(arena, &[t]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Inl(sum_ty, t))
    })
}

/// Construct right injection: inr [τ] t
#[no_mangle]
pub extern "C" fn tg_term_inr(sum_ty: TypeHandle, t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        if !valid_types(arena, &[sum_ty]) || !valid_terms(arena, &[t]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Inr(sum_ty, t))
    })
}
