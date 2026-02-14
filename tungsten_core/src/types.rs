//! Phase 1 Core Types
//!
//! Defines the type syntax for the Tungsten core calculus:
//! τ ::= Bool | Nat | Unit | Void | Prop | String | τ → τ | τ × τ | τ + τ | α | ∀α. τ | Eq τ t t | μα. τ
//!     | *τ | Ref τ  (Phase 3-Prep)

use serde::{Deserialize, Serialize};
use std::fmt;

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
}

impl Type {
    /// Construct a function type τ₁ → τ₂
    #[must_use]
    pub fn arrow(t1: Type, t2: Type) -> Type {
        Type::Arrow(Box::new(t1), Box::new(t2))
    }

    /// Construct a product type τ₁ × τ₂
    #[must_use]
    pub fn product(t1: Type, t2: Type) -> Type {
        Type::Product(Box::new(t1), Box::new(t2))
    }

    /// Construct a sum type τ₁ + τ₂
    #[must_use]
    pub fn sum(t1: Type, t2: Type) -> Type {
        Type::Sum(Box::new(t1), Box::new(t2))
    }

    /// Construct a forall type ∀α. τ
    pub fn forall(var: impl Into<String>, ty: Type) -> Type {
        Type::Forall(var.into(), Box::new(ty))
    }

    /// Construct an equality type Eq τ t₁ t₂
    #[must_use]
    pub fn eq(ty: Type, t1: crate::terms::Term, t2: crate::terms::Term) -> Type {
        Type::Eq(Box::new(ty), Box::new(t1), Box::new(t2))
    }

    /// Construct a recursive type μα. τ
    pub fn mu(var: impl Into<String>, ty: Type) -> Type {
        Type::Mu(var.into(), Box::new(ty))
    }

    /// Construct a pointer type *τ
    #[must_use]
    pub fn ptr(ty: Type) -> Type {
        Type::Ptr(Box::new(ty))
    }

    /// Construct a ref type Ref<τ>
    #[must_use]
    pub fn ref_ty(ty: Type) -> Type {
        Type::Ref(Box::new(ty))
    }

    /// Construct a deferred type application
    pub fn app(name: impl Into<String>, args: Vec<Type>) -> Type {
        Type::App(name.into(), args)
    }

    /// Construct an ADT type (flat enum)
    ///
    /// # Arguments
    /// - `name`: The ADT name
    /// - `type_args`: Type parameters  
    /// - `variants`: List of (`constructor_name`, `payload_type`) pairs
    pub fn adt(
        name: impl Into<String>,
        type_args: Vec<Type>,
        variants: Vec<(String, Type)>,
    ) -> Type {
        Type::Adt(name.into(), type_args, variants)
    }

    /// Substitute a type variable: τ[α := τ']
    #[must_use]
    pub fn substitute(&self, var: &str, replacement: &Type) -> Type {
        match self {
            Type::Bool => Type::Bool,
            Type::Nat => Type::Nat,
            Type::Unit => Type::Unit,
            Type::Void => Type::Void,
            Type::Prop => Type::Prop,
            Type::String => Type::String,

            Type::TyVar(v) if v == var => replacement.clone(),
            Type::TyVar(v) => Type::TyVar(v.clone()),

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

            Type::Forall(v, body) if v == var => {
                // Variable is shadowed, don't substitute
                Type::Forall(v.clone(), body.clone())
            }
            Type::Forall(v, body) => {
                // TODO: Handle capture-avoiding substitution properly
                Type::Forall(v.clone(), Box::new(body.substitute(var, replacement)))
            }

            Type::Eq(ty, t1, t2) => Type::Eq(
                Box::new(ty.substitute(var, replacement)),
                Box::new(t1.substitute_type(var, replacement)),
                Box::new(t2.substitute_type(var, replacement)),
            ),

            Type::Mu(v, body) if v == var => {
                // Variable is shadowed, don't substitute
                Type::Mu(v.clone(), body.clone())
            }
            Type::Mu(v, body) => {
                // TODO: Handle capture-avoiding substitution properly
                Type::Mu(v.clone(), Box::new(body.substitute(var, replacement)))
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
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Bool => write!(f, "Bool"),
            Type::Nat => write!(f, "Nat"),
            Type::Unit => write!(f, "Unit"),
            Type::Void => write!(f, "Void"),
            Type::Prop => write!(f, "Prop"),
            Type::String => write!(f, "String"),
            Type::TyVar(v) => write!(f, "{v}"),
            Type::Arrow(t1, t2) => write!(f, "({t1} → {t2})"),
            Type::Product(t1, t2) => write!(f, "({t1} × {t2})"),
            Type::Sum(t1, t2) => write!(f, "({t1} + {t2})"),
            Type::Forall(v, body) => write!(f, "∀{v}. {body}"),
            Type::Eq(ty, t1, t2) => write!(f, "Eq {ty} {t1} {t2}"),
            Type::Mu(v, body) => write!(f, "μ{v}. {body}"),
            Type::Ptr(inner) => write!(f, "Ptr<{inner}>"),
            Type::Ref(inner) => write!(f, "Ref<{inner}>"),
            Type::App(name, args) => {
                write!(f, "{name}<")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ">")
            }
            Type::Adt(name, type_args, variants) => {
                write!(f, "{name}[")?;
                // Type args
                if !type_args.is_empty() {
                    write!(f, "<")?;
                    for (i, arg) in type_args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{arg}")?;
                    }
                    write!(f, ">")?;
                }
                // Variants
                for (i, (ctor, payload)) in variants.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    if *payload == Type::Unit {
                        write!(f, "{ctor}")?;
                    } else {
                        write!(f, "{ctor}({payload})")?;
                    }
                }
                write!(f, "]")
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 1 Diagnostics: Detailed type display
// ─────────────────────────────────────────────────────────────────────────────

impl Type {
    /// Display the type in detailed form showing full structure.
    /// Useful for debugging type mismatches.
    #[must_use]
    pub fn display_detailed(&self) -> String {
        match self {
            Type::Bool => "Bool".to_string(),
            Type::Nat => "Nat".to_string(),
            Type::Unit => "Unit".to_string(),
            Type::Void => "Void".to_string(),
            Type::Prop => "Prop".to_string(),
            Type::String => "String".to_string(),
            Type::TyVar(v) => format!("TyVar({v})"),
            Type::Arrow(t1, t2) => {
                format!(
                    "Arrow({}, {})",
                    t1.display_detailed(),
                    t2.display_detailed()
                )
            }
            Type::Product(t1, t2) => {
                format!(
                    "Product({}, {})",
                    t1.display_detailed(),
                    t2.display_detailed()
                )
            }
            Type::Sum(t1, t2) => {
                format!("Sum({}, {})", t1.display_detailed(), t2.display_detailed())
            }
            Type::Forall(v, body) => {
                format!("Forall({}, {})", v, body.display_detailed())
            }
            Type::Eq(ty, t1, t2) => {
                format!("Eq({}, {}, {})", ty.display_detailed(), t1, t2)
            }
            Type::Mu(v, body) => {
                format!("Mu({}, {})", v, body.display_detailed())
            }
            Type::Ptr(inner) => format!("Ptr({})", inner.display_detailed()),
            Type::Ref(inner) => format!("Ref({})", inner.display_detailed()),
            Type::App(name, args) => {
                let arg_strs: Vec<String> = args.iter().map(Type::display_detailed).collect();
                format!("App({}, [{}])", name, arg_strs.join(", "))
            }
            Type::Adt(name, type_args, variants) => {
                let arg_strs: Vec<String> = type_args.iter().map(Type::display_detailed).collect();
                let var_strs: Vec<String> = variants
                    .iter()
                    .map(|(ctor, payload)| format!("({}, {})", ctor, payload.display_detailed()))
                    .collect();
                format!(
                    "Adt({}, [{}], [{}])",
                    name,
                    arg_strs.join(", "),
                    var_strs.join(", ")
                )
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 2: α-Equivalent Type Equality
// ─────────────────────────────────────────────────────────────────────────────

use std::collections::HashMap;

/// Check if two types are α-equivalent (equal up to renaming of bound variables).
///
/// This handles:
/// - μα.F ≡ μβ.F[β/α] (μ-types with different bound variable names)
/// - ∀α.τ ≡ ∀β.τ[β/α] (forall types with different bound variable names)
#[must_use]
pub fn types_equal_alpha(a: &Type, b: &Type) -> bool {
    types_equal_with_env(a, b, &mut HashMap::new())
}

/// Check type equality with an environment mapping bound variables.
///
/// The `env` maps bound variables in `a` to their corresponding bound variables in `b`.
/// Only populated when descending into Mu or Forall binders—free variables
/// are compared by name equality, not tracked here.
fn types_equal_with_env(a: &Type, b: &Type, env: &mut HashMap<String, String>) -> bool {
    match (a, b) {
        // Base types
        (Type::Bool, Type::Bool) => true,
        (Type::Nat, Type::Nat) => true,
        (Type::Unit, Type::Unit) => true,
        (Type::Void, Type::Void) => true,
        (Type::Prop, Type::Prop) => true,
        (Type::String, Type::String) => true,

        // Type variables: check if bound or free
        (Type::TyVar(v1), Type::TyVar(v2)) => {
            // If v1 is a bound variable (in env), check it maps to v2
            // Otherwise, they're free variables and must be equal by name
            env.get(v1).map_or(v1 == v2, |mapped| mapped == v2)
        }

        // 0-arity App is equivalent to TyVar (both represent the same named type)
        // This handles the asymmetry where one side has TyVar("X") and other has App("X", [])
        (Type::TyVar(v), Type::App(name, args)) if args.is_empty() => {
            // If v is bound, check it maps to name
            // Otherwise, names must match
            env.get(v).map_or(v == name, |mapped| mapped == name)
        }
        (Type::App(name, args), Type::TyVar(v)) if args.is_empty() => {
            // Symmetric case
            env.get(v).map_or(v == name, |mapped| mapped == name)
        }

        // TyVar("X") vs Mu("α_X", body) - handles normalization depth asymmetry
        // When one side has an unexpanded type reference and the other has the Mu encoding
        // of that type, they should be considered equal.
        // This uses our naming convention: Mu variable "α_X" corresponds to type "X".
        // Also handles the self-hosted compiler convention where Mu variable "X" directly
        // matches type name "X" (no α_ prefix).
        (Type::TyVar(v), Type::Mu(mu_var, _body)) => {
            // Check if the Mu variable follows the α_ naming convention
            if let Some(type_name) = mu_var.strip_prefix("α_") {
                // If v is bound, check mapped name; otherwise v must match type_name
                env.get(v)
                    .map_or(v == type_name, |mapped| mapped == type_name)
            } else {
                // Self-hosted compiler convention: plain name match
                env.get(v).map_or(v == mu_var, |mapped| mapped == mu_var)
            }
        }
        (Type::Mu(mu_var, _body), Type::TyVar(v)) => {
            // Symmetric case
            if let Some(type_name) = mu_var.strip_prefix("α_") {
                env.get(v)
                    .map_or(v == type_name, |mapped| mapped == type_name)
            } else {
                // Self-hosted compiler convention: plain name match
                env.get(v).map_or(v == mu_var, |mapped| mapped == mu_var)
            }
        }

        // Arrow types
        (Type::Arrow(a1, a2), Type::Arrow(b1, b2)) => {
            types_equal_with_env(a1, b1, env) && types_equal_with_env(a2, b2, env)
        }

        // Product types
        (Type::Product(a1, a2), Type::Product(b1, b2)) => {
            types_equal_with_env(a1, b1, env) && types_equal_with_env(a2, b2, env)
        }

        // Sum types
        (Type::Sum(a1, a2), Type::Sum(b1, b2)) => {
            types_equal_with_env(a1, b1, env) && types_equal_with_env(a2, b2, env)
        }

        // μ-types: bind the recursion variables and compare bodies
        (Type::Mu(v1, body1), Type::Mu(v2, body2)) => {
            // Temporarily bind v1 → v2 for body comparison
            let old = env.insert(v1.clone(), v2.clone());
            let result = types_equal_with_env(body1, body2, env);
            // Restore previous binding (if any)
            if let Some(prev) = old {
                env.insert(v1.clone(), prev);
            } else {
                env.remove(v1);
            }
            result
        }

        // Forall types: bind the type variables and compare bodies
        (Type::Forall(v1, body1), Type::Forall(v2, body2)) => {
            // Temporarily bind v1 → v2 for body comparison
            let old = env.insert(v1.clone(), v2.clone());
            let result = types_equal_with_env(body1, body2, env);
            // Restore previous binding (if any)
            if let Some(prev) = old {
                env.insert(v1.clone(), prev);
            } else {
                env.remove(v1);
            }
            result
        }

        // Equality types (quasi-dependent)
        (Type::Eq(ty1, t1a, t1b), Type::Eq(ty2, t2a, t2b)) => {
            // For Eq types, compare the type component with α-equivalence
            // For the term components, we use structural equality for now
            // (Full term α-equivalence would require more work)
            types_equal_with_env(ty1, ty2, env) && t1a == t2a && t1b == t2b
        }

        // Pointer types
        (Type::Ptr(inner1), Type::Ptr(inner2)) => types_equal_with_env(inner1, inner2, env),

        // Ref types
        (Type::Ref(inner1), Type::Ref(inner2)) => types_equal_with_env(inner1, inner2, env),

        // Type applications (deferred type applications)
        (Type::App(name1, args1), Type::App(name2, args2)) => {
            name1 == name2
                && args1.len() == args2.len()
                && args1
                    .iter()
                    .zip(args2.iter())
                    .all(|(a1, a2)| types_equal_with_env(a1, a2, env))
        }

        // ADT types (flat enum representation)
        (Type::Adt(name1, args1, vars1), Type::Adt(name2, args2, vars2)) => {
            name1 == name2
                && args1.len() == args2.len()
                && vars1.len() == vars2.len()
                && args1
                    .iter()
                    .zip(args2.iter())
                    .all(|(a1, a2)| types_equal_with_env(a1, a2, env))
                && vars1
                    .iter()
                    .zip(vars2.iter())
                    .all(|((ctor1, pay1), (ctor2, pay2))| {
                        ctor1 == ctor2 && types_equal_with_env(pay1, pay2, env)
                    })
        }

        // All other combinations are not equal
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_display() {
        assert_eq!(Type::Bool.to_string(), "Bool");
        assert_eq!(
            Type::arrow(Type::Nat, Type::Bool).to_string(),
            "(Nat → Bool)"
        );
        assert_eq!(
            Type::product(Type::Bool, Type::Nat).to_string(),
            "(Bool × Nat)"
        );
        assert_eq!(
            Type::sum(Type::Unit, Type::Void).to_string(),
            "(Unit + Void)"
        );
        assert_eq!(
            Type::forall("α", Type::TyVar("α".into())).to_string(),
            "∀α. α"
        );
    }

    #[test]
    fn test_type_substitution() {
        let ty = Type::TyVar("α".into());
        let result = ty.substitute("α", &Type::Nat);
        assert_eq!(result, Type::Nat);

        let arrow = Type::arrow(Type::TyVar("α".into()), Type::TyVar("α".into()));
        let result = arrow.substitute("α", &Type::Bool);
        assert_eq!(result, Type::arrow(Type::Bool, Type::Bool));
    }

    #[test]
    fn test_forall_shadowing() {
        // ∀α. α should not substitute inner α
        let ty = Type::forall("α", Type::TyVar("α".into()));
        let result = ty.substitute("α", &Type::Nat);
        assert_eq!(result, Type::forall("α", Type::TyVar("α".into())));
    }

    #[test]
    fn test_free_type_vars() {
        let ty = Type::arrow(Type::TyVar("α".into()), Type::TyVar("β".into()));
        let free = ty.free_type_vars();
        assert!(free.contains("α"));
        assert!(free.contains("β"));
        assert_eq!(free.len(), 2);

        // Forall binds α
        let ty = Type::forall(
            "α",
            Type::arrow(Type::TyVar("α".into()), Type::TyVar("β".into())),
        );
        let free = ty.free_type_vars();
        assert!(!free.contains("α"));
        assert!(free.contains("β"));
        assert_eq!(free.len(), 1);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // α-Equivalence Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_mu_alpha_equivalence_same_var() {
        // μα. Unit + α  ≡  μα. Unit + α
        let ty1 = Type::mu("alpha", Type::sum(Type::Unit, Type::TyVar("alpha".into())));
        let ty2 = Type::mu("alpha", Type::sum(Type::Unit, Type::TyVar("alpha".into())));
        assert!(types_equal_alpha(&ty1, &ty2));
    }

    #[test]
    fn test_mu_alpha_equivalence_different_var() {
        // μα. Unit + α  ≡  μβ. Unit + β
        let ty1 = Type::mu("alpha", Type::sum(Type::Unit, Type::TyVar("alpha".into())));
        let ty2 = Type::mu("beta", Type::sum(Type::Unit, Type::TyVar("beta".into())));
        assert!(types_equal_alpha(&ty1, &ty2));
    }

    #[test]
    fn test_mu_not_equal_different_structure() {
        // μα. Unit + α  ≢  μα. Nat + α
        let ty1 = Type::mu("alpha", Type::sum(Type::Unit, Type::TyVar("alpha".into())));
        let ty2 = Type::mu("alpha", Type::sum(Type::Nat, Type::TyVar("alpha".into())));
        assert!(!types_equal_alpha(&ty1, &ty2));
    }

    #[test]
    fn test_mu_nested_alpha_equivalence() {
        // μα. μβ. α × β  ≡  μx. μy. x × y
        let ty1 = Type::mu(
            "alpha",
            Type::mu(
                "beta",
                Type::product(Type::TyVar("alpha".into()), Type::TyVar("beta".into())),
            ),
        );
        let ty2 = Type::mu(
            "x",
            Type::mu(
                "y",
                Type::product(Type::TyVar("x".into()), Type::TyVar("y".into())),
            ),
        );
        assert!(types_equal_alpha(&ty1, &ty2));
    }

    #[test]
    fn test_forall_alpha_equivalence() {
        // ∀α. α → α  ≡  ∀β. β → β
        let ty1 = Type::forall(
            "alpha",
            Type::arrow(Type::TyVar("alpha".into()), Type::TyVar("alpha".into())),
        );
        let ty2 = Type::forall(
            "beta",
            Type::arrow(Type::TyVar("beta".into()), Type::TyVar("beta".into())),
        );
        assert!(types_equal_alpha(&ty1, &ty2));
    }

    #[test]
    fn test_free_vs_bound_not_equal() {
        // μα. α  ≢  μα. β  (where β is free)
        let ty1 = Type::mu("alpha", Type::TyVar("alpha".into()));
        let ty2 = Type::mu("alpha", Type::TyVar("beta".into()));
        assert!(!types_equal_alpha(&ty1, &ty2));
    }

    #[test]
    fn test_base_types_equal() {
        assert!(types_equal_alpha(&Type::Nat, &Type::Nat));
        assert!(types_equal_alpha(&Type::Bool, &Type::Bool));
        assert!(types_equal_alpha(&Type::Unit, &Type::Unit));
        assert!(types_equal_alpha(&Type::String, &Type::String));
        assert!(!types_equal_alpha(&Type::Nat, &Type::Bool));
    }

    #[test]
    fn test_complex_list_type_equivalence() {
        // List<Nat> representation: μα. Unit + (Nat × α)
        // With different bound var names should be equal
        let list1 = Type::mu(
            "α_List",
            Type::sum(
                Type::Unit,
                Type::product(Type::Nat, Type::TyVar("α_List".into())),
            ),
        );
        let list2 = Type::mu(
            "rec",
            Type::sum(
                Type::Unit,
                Type::product(Type::Nat, Type::TyVar("rec".into())),
            ),
        );
        assert!(types_equal_alpha(&list1, &list2));
    }

    #[test]
    fn test_display_detailed() {
        let ty = Type::mu(
            "α",
            Type::sum(
                Type::Unit,
                Type::product(Type::Nat, Type::TyVar("α".into())),
            ),
        );
        let detailed = ty.display_detailed();
        assert!(detailed.contains("Mu(α"));
        assert!(detailed.contains("Sum("));
        assert!(detailed.contains("Product("));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // TyVar vs 0-arity App Equivalence Tests (ADR 1.2.26)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_tyvar_equals_zero_arity_app() {
        // TyVar("X") should equal App("X", [])
        let ty1 = Type::TyVar("CodegenType".into());
        let ty2 = Type::app("CodegenType", vec![]);
        assert!(
            types_equal_alpha(&ty1, &ty2),
            "TyVar(X) should equal App(X, [])"
        );
    }

    #[test]
    fn test_zero_arity_app_equals_tyvar() {
        // Symmetric case: App("X", []) should equal TyVar("X")
        let ty1 = Type::app("TypeExpr", vec![]);
        let ty2 = Type::TyVar("TypeExpr".into());
        assert!(
            types_equal_alpha(&ty1, &ty2),
            "App(X, []) should equal TyVar(X)"
        );
    }

    #[test]
    fn test_tyvar_vs_app_in_list_arg() {
        // List<TyVar("T")> should equal List<App("T", [])>
        let ty1 = Type::app("List", vec![Type::TyVar("CodegenType".into())]);
        let ty2 = Type::app("List", vec![Type::app("CodegenType", vec![])]);
        assert!(
            types_equal_alpha(&ty1, &ty2),
            "List<TyVar(T)> should equal List<App(T, [])>"
        );
    }

    #[test]
    fn test_tyvar_vs_app_in_mu_body() {
        // Inside a Mu body, TyVar("X") should equal App("X", [])
        let ty1 = Type::mu(
            "α_List",
            Type::sum(
                Type::Unit,
                Type::product(Type::TyVar("TypeExpr".into()), Type::TyVar("α_List".into())),
            ),
        );
        let ty2 = Type::mu(
            "α_List",
            Type::sum(
                Type::Unit,
                Type::product(Type::app("TypeExpr", vec![]), Type::TyVar("α_List".into())),
            ),
        );
        assert!(
            types_equal_alpha(&ty1, &ty2),
            "TyVar vs App(_, []) should be equal inside Mu bodies"
        );
    }

    #[test]
    fn test_tyvar_vs_app_in_product() {
        // In a product type: (TyVar("A") × TyVar("B")) should equal (App("A", []) × App("B", []))
        let ty1 = Type::product(Type::TyVar("A".into()), Type::TyVar("B".into()));
        let ty2 = Type::product(Type::app("A", vec![]), Type::app("B", vec![]));
        assert!(
            types_equal_alpha(&ty1, &ty2),
            "Products with TyVar vs App(_, []) should be equal"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // TyVar vs Mu Equivalence Tests (ADR 1.2.26 Phase 3B)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_tyvar_equals_mu_with_alpha_prefix() {
        // TyVar("TypeExpr") should equal Mu("α_TypeExpr", body)
        // This handles the normalization depth asymmetry where one side references
        // a type by name and the other has its recursive encoding.
        let ty1 = Type::TyVar("TypeExpr".into());
        let ty2 = Type::mu(
            "α_TypeExpr",
            Type::sum(Type::Unit, Type::TyVar("α_TypeExpr".into())),
        );
        assert!(
            types_equal_alpha(&ty1, &ty2),
            "TyVar(X) should equal Mu(α_X, body)"
        );
    }

    #[test]
    fn test_mu_equals_tyvar_symmetric() {
        // Symmetric case: Mu("α_Stmt", body) should equal TyVar("Stmt")
        let ty1 = Type::mu(
            "α_Stmt",
            Type::sum(Type::Unit, Type::TyVar("α_Stmt".into())),
        );
        let ty2 = Type::TyVar("Stmt".into());
        assert!(
            types_equal_alpha(&ty1, &ty2),
            "Mu(α_X, body) should equal TyVar(X)"
        );
    }

    #[test]
    fn test_tyvar_not_equal_mu_wrong_prefix() {
        // TyVar("Foo") should NOT equal Mu("β_Foo", body) - wrong prefix
        let ty1 = Type::TyVar("Foo".into());
        let ty2 = Type::mu("β_Foo", Type::sum(Type::Unit, Type::TyVar("β_Foo".into())));
        assert!(
            !types_equal_alpha(&ty1, &ty2),
            "TyVar(X) should not equal Mu(β_X, body) - only α_ prefix works"
        );
    }

    #[test]
    fn test_tyvar_not_equal_mu_name_mismatch() {
        // TyVar("Foo") should NOT equal Mu("α_Bar", body) - different names
        let ty1 = Type::TyVar("Foo".into());
        let ty2 = Type::mu("α_Bar", Type::sum(Type::Unit, Type::TyVar("α_Bar".into())));
        assert!(
            !types_equal_alpha(&ty1, &ty2),
            "TyVar(Foo) should not equal Mu(α_Bar, body)"
        );
    }

    #[test]
    fn test_tyvar_vs_mu_in_list_element() {
        // List<TyVar("TypeExpr")> should equal List<Mu("α_TypeExpr", ...)>
        // This is the actual pattern causing L2 errors
        let type_expr_as_tyvar = Type::TyVar("TypeExpr".into());
        let type_expr_as_mu = Type::mu(
            "α_TypeExpr",
            Type::sum(
                Type::Unit,
                Type::product(Type::Nat, Type::TyVar("α_TypeExpr".into())),
            ),
        );

        let list_with_tyvar = Type::mu(
            "α_List",
            Type::sum(
                Type::Unit,
                Type::product(type_expr_as_tyvar, Type::TyVar("α_List".into())),
            ),
        );
        let list_with_mu = Type::mu(
            "α_List",
            Type::sum(
                Type::Unit,
                Type::product(type_expr_as_mu, Type::TyVar("α_List".into())),
            ),
        );

        assert!(
            types_equal_alpha(&list_with_tyvar, &list_with_mu),
            "List<TyVar(T)> should equal List<Mu(α_T, ...)>"
        );
    }
}
