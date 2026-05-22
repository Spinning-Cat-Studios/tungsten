//! Type introspection predicates for FFI.
//!
//! Provides C-compatible type predicate functions (`tg_type_is_*`).
//! Type accessors, substitution, and debug utilities are in `types_accessors.rs`.

use crate::types::Type;

use crate::ffi::{with_arena, TypeHandle};
// ============================================================================
// Type Tag (discriminant)
// ============================================================================

/// Return a numeric tag identifying the top-level variant of a type.
///
/// Tags:
///   0 = Nat, 1 = Bool, 2 = String, 3 = Unit, 4 = Void, 5 = Prop,
///   6 = Arrow, 7 = Product, 8 = Sum, 9 = TyVar, 10 = Forall,
///   11 = Mu, 12 = Eq, 13 = Ref, 14 = Ptr, 15 = App,
///   99 = unknown / invalid handle
#[no_mangle]
pub extern "C" fn tg_type_tag(ty: TypeHandle) -> u64 {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Nat) => 0,
            Some(Type::Bool) => 1,
            Some(Type::String) => 2,
            Some(Type::Unit) => 3,
            Some(Type::Void) => 4,
            Some(Type::Prop) => 5,
            Some(Type::Arrow(_, _)) => 6,
            Some(Type::Product(_, _)) => 7,
            Some(Type::Sum(_, _)) => 8,
            Some(Type::TyVar(_)) => 9,
            Some(Type::Forall(_, _)) => 10,
            Some(Type::Mu(_, _)) => 11,
            Some(Type::Eq(_, _, _)) => 12,
            Some(Type::Ref(_)) => 13,
            Some(Type::Ptr(_)) => 14,
            Some(Type::App(_, _)) => 15,
            Some(Type::Adt(_, _, _)) => 16,
            Some(Type::Error) => 99,
            None => 99,
        }
    })
}

// ============================================================================
// Type Predicates (Phase 3C-5)
// ============================================================================

/// Check if a type is a μ-type (recursive type)
#[no_mangle]
pub extern "C" fn tg_type_is_mu(ty: TypeHandle) -> bool {
    with_arena!(|arena| { matches!(arena.get_type(ty), Some(Type::Mu(_, _))) })
}

/// Check if a type is a sum type
#[no_mangle]
pub extern "C" fn tg_type_is_sum(ty: TypeHandle) -> bool {
    with_arena!(|arena| { matches!(arena.get_type(ty), Some(Type::Sum(_, _))) })
}

/// Check if a type is a product type
#[no_mangle]
pub extern "C" fn tg_type_is_product(ty: TypeHandle) -> bool {
    with_arena!(|arena| { matches!(arena.get_type(ty), Some(Type::Product(_, _))) })
}

/// Check if a type is an arrow (function) type
#[no_mangle]
pub extern "C" fn tg_type_is_arrow(ty: TypeHandle) -> bool {
    with_arena!(|arena| { matches!(arena.get_type(ty), Some(Type::Arrow(_, _))) })
}

/// Check if a type is an equality type (Eq τ t₁ t₂)
#[no_mangle]
pub extern "C" fn tg_type_is_eq(ty: TypeHandle) -> bool {
    with_arena!(|arena| { matches!(arena.get_type(ty), Some(Type::Eq(_, _, _))) })
}

/// Check if a type is a forall type (∀α. τ)
#[no_mangle]
pub extern "C" fn tg_type_is_forall(ty: TypeHandle) -> bool {
    with_arena!(|arena| { matches!(arena.get_type(ty), Some(Type::Forall(_, _))) })
}

/// Check if a type is a type variable (named type like record names).
#[no_mangle]
pub extern "C" fn tg_type_is_tyvar(ty: TypeHandle) -> bool {
    with_arena!(|arena| { matches!(arena.get_type(ty), Some(Type::TyVar(_))) })
}

/// Check if a type is a type application (parametric type like List<T>).
#[no_mangle]
pub extern "C" fn tg_type_is_app(ty: TypeHandle) -> bool {
    with_arena!(|arena| { matches!(arena.get_type(ty), Some(Type::App(_, _))) })
}
