//! Phase 1 Core Types
//!
//! Defines the type syntax for the Tungsten core calculus:
//! τ ::= Bool | Nat | Unit | Void | Prop | String | τ → τ | τ × τ | τ + τ | α | ∀α. τ | Eq τ t t | μα. τ
//!     | *τ | Ref τ  (Phase 3-Prep)

use serde::{Deserialize, Serialize};

/// A type variable name (e.g., α, β)
pub type TyVar = String;

/// Phase 1 Core Types
///
/// τ ::= Bool
///     | Nat
///     | Unit
///     | Void
///     | Prop
///     | String
///     | τ → τ
///     | τ × τ
///     | τ + τ
///     | α
///     | ∀α. τ
///     | Eq τ t t
///     | μα. τ
///     | *τ (Pointer, Phase 3-Prep)
///     | Ref τ (Mutable reference, Phase 3-Prep)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Type {
    /// Booleans
    Bool,

    /// Natural numbers (primitive in Phase 1)
    Nat,

    /// Unit type - single inhabitant ()
    Unit,

    /// Empty type - no inhabitants (logical False)
    Void,

    /// Universe of propositions
    Prop,

    /// String type (Phase 2A)
    String,

    /// Function type τ₁ → τ₂
    Arrow(Box<Type>, Box<Type>),

    /// Product type τ₁ × τ₂
    Product(Box<Type>, Box<Type>),

    /// Sum type τ₁ + τ₂
    Sum(Box<Type>, Box<Type>),

    /// Type variable α
    TyVar(TyVar),

    /// For-all type ∀α. τ (rank-1 polymorphism)
    Forall(TyVar, Box<Type>),

    /// Equality type Eq τ t₁ t₂ (quasi-dependent)
    /// - τ is the type of both terms
    /// - t₁ and t₂ are the terms being compared
    Eq(Box<Type>, Box<crate::terms::Term>, Box<crate::terms::Term>),

    /// Recursive type μα. τ (Phase 2A)
    /// - α is bound in τ
    /// - Represents the least fixed point
    Mu(TyVar, Box<Type>),

    /// Pointer type *τ (Phase 3-Prep, FFI)
    /// Raw pointer for interop with C code
    Ptr(Box<Type>),

    /// Ref type Ref<τ> (Phase 3-Prep)
    /// Mutable reference cell
    Ref(Box<Type>),

    /// Deferred type application (elaboration-only)
    ///
    /// Used during elaboration when a generic type is referenced before its
    /// definition is fully elaborated. For example, when `Tree<T>` references
    /// `Forest<T>` and `Forest` is still a stub, we store `App("Forest", [T])`
    /// instead of losing the type arguments.
    ///
    /// This variant is resolved in Phase 1d after all types are elaborated.
    /// It should never appear in the final Core output.
    App(String, Vec<Type>),

    /// Algebraic Data Type (flat enum representation)
    ///
    /// For ADTs with n >= 3 constructors, we use a flat representation:
    /// - `{ i32 tag, [max_payload x i8] data }`
    /// - Enables O(1) variant dispatch via switch instead of O(n) nested branches
    ///
    /// Fields:
    /// - name: The ADT name (e.g., "`TokenKind`")
    /// - `type_args`: Type parameters (e.g., [T] for Option<T>)
    /// - variants: List of (`constructor_name`, `payload_type`) pairs
    ///
    /// See ADR 2.2.26 for rationale.
    Adt(String, Vec<Type>, Vec<(String, Type)>),

    /// Error/poison type for cascade prevention.
    ///
    /// Unifies silently with any type to prevent secondary error diagnostics.
    /// Introduced when elaboration of a definition body fails — the definition's
    /// type is set to `Error` instead of leaving it as `INVALID_HANDLE`, so that
    /// downstream callers don't each generate a "type mismatch" error.
    ///
    /// TypeError is a poison value, not a wildcard: it suppresses new error
    /// generation but never converts an error path into a success path.
    Error,
}

impl Type {
    /// Substitute a type variable: τ[α := τ']
    #[must_use]
    pub fn substitute(&self, var: &str, replacement: &Type) -> Type {
        match self {
            Type::Bool
            | Type::Nat
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::Error => self.clone(),

            Type::TyVar(v) => {
                if v == var {
                    replacement.clone()
                } else {
                    Type::TyVar(v.clone())
                }
            }

            Type::Arrow(t1, t2) => Type::arrow(
                t1.substitute(var, replacement),
                t2.substitute(var, replacement),
            ),

            Type::Product(t1, t2) => Type::product(
                t1.substitute(var, replacement),
                t2.substitute(var, replacement),
            ),

            Type::Sum(t1, t2) => Type::sum(
                t1.substitute(var, replacement),
                t2.substitute(var, replacement),
            ),

            Type::Forall(v, body) => {
                let new_body = if v == var {
                    body.as_ref().clone()
                } else {
                    body.substitute(var, replacement)
                };
                Type::Forall(v.clone(), Box::new(new_body))
            }

            Type::Eq(ty, t1, t2) => Type::Eq(
                Box::new(ty.substitute(var, replacement)),
                Box::new(t1.substitute_type(var, replacement)),
                Box::new(t2.substitute_type(var, replacement)),
            ),

            Type::Mu(v, body) => {
                let new_body = if v == var {
                    body.as_ref().clone()
                } else {
                    body.substitute(var, replacement)
                };
                Type::Mu(v.clone(), Box::new(new_body))
            }

            Type::Ptr(inner) => Type::ptr(inner.substitute(var, replacement)),
            Type::Ref(inner) => Type::ref_ty(inner.substitute(var, replacement)),

            Type::App(name, args) => Type::app(
                name.clone(),
                args.iter()
                    .map(|a| a.substitute(var, replacement))
                    .collect(),
            ),

            Type::Adt(name, type_args, variants) => Type::adt(
                name.clone(),
                type_args
                    .iter()
                    .map(|a| a.substitute(var, replacement))
                    .collect(),
                variants
                    .iter()
                    .map(|(ctor, payload)| (ctor.clone(), payload.substitute(var, replacement)))
                    .collect(),
            ),
        }
    }

    /// Get free type variables in this type
    #[must_use]
    pub fn free_type_vars(&self) -> std::collections::HashSet<TyVar> {
        use std::collections::HashSet;
        match self {
            Type::Bool | Type::Nat | Type::Unit | Type::Void | Type::Prop | Type::String => {
                HashSet::new()
            }

            Type::TyVar(v) => {
                let mut set = HashSet::new();
                set.insert(v.clone());
                set
            }

            Type::Arrow(t1, t2) | Type::Product(t1, t2) | Type::Sum(t1, t2) => {
                let mut set = t1.free_type_vars();
                set.extend(t2.free_type_vars());
                set
            }

            Type::Forall(v, body) | Type::Mu(v, body) => {
                let mut set = body.free_type_vars();
                set.remove(v);
                set
            }

            Type::Eq(ty, t1, t2) => {
                let mut set = ty.free_type_vars();
                set.extend(t1.free_type_vars());
                set.extend(t2.free_type_vars());
                set
            }

            Type::Ptr(inner) | Type::Ref(inner) => inner.free_type_vars(),

            Type::App(_, args) => {
                let mut set = std::collections::HashSet::new();
                for arg in args {
                    set.extend(arg.free_type_vars());
                }
                set
            }

            Type::Adt(_, type_args, variants) => {
                let mut set = std::collections::HashSet::new();
                for arg in type_args {
                    set.extend(arg.free_type_vars());
                }
                for (_, payload) in variants {
                    set.extend(payload.free_type_vars());
                }
                set
            }

            // Error type has no free variables
            Type::Error => HashSet::new(),
        }
    }

    /// Check if this type is well-formed given a set of type variables in scope
    #[must_use]
    pub fn is_well_formed(&self, type_vars: &std::collections::HashSet<TyVar>) -> bool {
        match self {
            Type::Bool | Type::Nat | Type::Unit | Type::Void | Type::Prop | Type::String => true,

            Type::TyVar(v) => type_vars.contains(v),

            Type::Arrow(t1, t2) | Type::Product(t1, t2) | Type::Sum(t1, t2) => {
                t1.is_well_formed(type_vars) && t2.is_well_formed(type_vars)
            }

            Type::Forall(v, body) | Type::Mu(v, body) => {
                let mut extended = type_vars.clone();
                extended.insert(v.clone());
                body.is_well_formed(&extended)
            }

            Type::Eq(ty, _t1, _t2) => {
                // TODO: Check that t1 and t2 have type ty
                ty.is_well_formed(type_vars)
            }

            Type::Ptr(inner) | Type::Ref(inner) => inner.is_well_formed(type_vars),

            Type::App(_, args) => args.iter().all(|a| a.is_well_formed(type_vars)),

            Type::Adt(_, type_args, variants) => {
                type_args.iter().all(|a| a.is_well_formed(type_vars))
                    && variants
                        .iter()
                        .all(|(_, payload)| payload.is_well_formed(type_vars))
            }

            // Error type is always "well-formed" — it's a sentinel, not a real type
            Type::Error => true,
        }
    }

    /// Count the total number of nodes in this type tree.
    #[must_use]
    pub fn node_count(&self) -> usize {
        match self {
            Type::Bool
            | Type::Nat
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::TyVar(_)
            | Type::Error => 1,

            Type::Arrow(t1, t2) | Type::Product(t1, t2) | Type::Sum(t1, t2) => {
                1 + t1.node_count() + t2.node_count()
            }

            Type::Forall(_, body) | Type::Mu(_, body) => 1 + body.node_count(),

            Type::Eq(ty, _, _) => 1 + ty.node_count(),

            Type::Ptr(inner) | Type::Ref(inner) => 1 + inner.node_count(),

            Type::App(_, args) => 1 + args.iter().map(Type::node_count).sum::<usize>(),

            Type::Adt(_, type_args, variants) => {
                1 + type_args.iter().map(Type::node_count).sum::<usize>()
                    + variants.iter().map(|(_, p)| p.node_count()).sum::<usize>()
            }
        }
    }

    /// Compute the maximum depth of this type tree.
    #[must_use]
    pub fn depth(&self) -> usize {
        match self {
            Type::Bool
            | Type::Nat
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::TyVar(_)
            | Type::Error => 1,

            Type::Arrow(t1, t2) | Type::Product(t1, t2) | Type::Sum(t1, t2) => {
                1 + t1.depth().max(t2.depth())
            }

            Type::Forall(_, body) | Type::Mu(_, body) => 1 + body.depth(),

            Type::Eq(ty, _, _) => 1 + ty.depth(),

            Type::Ptr(inner) | Type::Ref(inner) => 1 + inner.depth(),

            Type::App(_, args) => 1 + args.iter().map(Type::depth).max().unwrap_or(0),

            Type::Adt(_, type_args, variants) => {
                let arg_max = type_args.iter().map(Type::depth).max().unwrap_or(0);
                let var_max = variants.iter().map(|(_, p)| p.depth()).max().unwrap_or(0);
                1 + arg_max.max(var_max)
            }
        }
    }
}

mod constructors;
mod display;
mod equality;
mod reconstruction;
mod tyvar_ops;
pub use equality::types_equal_alpha;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tyvar_ops_tests;
