//! Type component accessors.
//!
//! Provides C-compatible functions for decomposing types (get_* accessors).
//! Type variable/app accessors, substitution, and debug are in `accessors_introspection.rs`.
//! Type predicates (is_*) are in `predicates.rs`.

use crate::types::Type;

use crate::ffi::{with_arena, TypeHandle, INVALID_HANDLE};
// ============================================================================
// Type Component Accessors
// ============================================================================

/// Get the body of a μ-type. Returns `INVALID_HANDLE` if not a μ-type.
#[no_mangle]
pub extern "C" fn tg_type_get_mu_body(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Mu(_, body)) => arena.alloc_type((**body).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the left component of a sum type. Returns `INVALID_HANDLE` if not a sum.
#[no_mangle]
pub extern "C" fn tg_type_get_sum_left(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        if let Some(Type::Sum(left, _)) = arena.get_type(ty) {
            arena.alloc_type((**left).clone())
        } else {
            #[cfg(debug_assertions)]
            if std::env::var("TG_DEBUG_TYPES").is_ok() {
                eprintln!("[warn] tg_type_get_sum_left called on non-sum: handle={ty}");
            }
            INVALID_HANDLE
        }
    })
}

/// Get the right component of a sum type. Returns `INVALID_HANDLE` if not a sum.
#[no_mangle]
pub extern "C" fn tg_type_get_sum_right(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        if let Some(Type::Sum(_, right)) = arena.get_type(ty) {
            arena.alloc_type((**right).clone())
        } else {
            #[cfg(debug_assertions)]
            if std::env::var("TG_DEBUG_TYPES").is_ok() {
                eprintln!("[warn] tg_type_get_sum_right called on non-sum: handle={ty}");
            }
            INVALID_HANDLE
        }
    })
}

/// Get the left component of a product type. Returns `INVALID_HANDLE` if not a product.
#[no_mangle]
pub extern "C" fn tg_type_get_product_left(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Product(left, _)) => arena.alloc_type((**left).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the right component of a product type. Returns `INVALID_HANDLE` if not a product.
#[no_mangle]
pub extern "C" fn tg_type_get_product_right(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Product(_, right)) => arena.alloc_type((**right).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the domain of an arrow type. Returns `INVALID_HANDLE` if not an arrow.
#[no_mangle]
pub extern "C" fn tg_type_get_arrow_domain(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Arrow(domain, _)) => arena.alloc_type((**domain).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the codomain of an arrow type. Returns `INVALID_HANDLE` if not an arrow.
#[no_mangle]
pub extern "C" fn tg_type_get_arrow_codomain(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Arrow(_, codomain)) => arena.alloc_type((**codomain).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the type component of an equality type. Returns `INVALID_HANDLE` if not Eq.
#[no_mangle]
pub extern "C" fn tg_type_get_eq_type(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Eq(inner_ty, _, _)) => arena.alloc_type((**inner_ty).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the LHS term of an equality type. Returns `INVALID_HANDLE` if not Eq.
#[no_mangle]
pub extern "C" fn tg_type_get_eq_lhs(ty: TypeHandle) -> super::super::TermHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Eq(_, lhs, _)) => arena.alloc_term((**lhs).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the RHS term of an equality type. Returns `INVALID_HANDLE` if not Eq.
#[no_mangle]
pub extern "C" fn tg_type_get_eq_rhs(ty: TypeHandle) -> super::super::TermHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Eq(_, _, rhs)) => arena.alloc_term((**rhs).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the body of a forall type. Returns `INVALID_HANDLE` if not Forall.
/// Note: The body still has the free type variable - full substitution
/// requires additional support.
#[no_mangle]
pub extern "C" fn tg_type_get_forall_body(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Forall(_, body)) => arena.alloc_type((**body).clone()),
            _ => INVALID_HANDLE,
        }
    })
}
