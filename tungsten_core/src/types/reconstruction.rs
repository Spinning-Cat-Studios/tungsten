//! Type reconstruction and structural-traversal helpers.
//!
//! Used by type-recursive traversals to visit or rebuild the same variant
//! from its children, without every caller re-enumerating all ~15 variants.
//! [`Type::children`] is the read-only counterpart (collectors, predicate
//! folds); [`Type::map_children`] is the transforming counterpart (resolvers,
//! substitutions). The narrower `reconstruct_*` helpers rebuild one variant
//! group from already-transformed children.

use crate::types::Type;

impl Type {
    /// The direct child *types* of `self`, in structural order.
    ///
    /// Yields only immediate `Type` sub-terms — not constructor/variant
    /// names, bound-variable names, or the `Eq` witness terms (none of which
    /// are `Type`s). Terminal variants
    /// (`Bool`/`Nat`/`Unit`/`Void`/`Prop`/`String`/`TyVar`/`Error`) yield
    /// nothing. Use for read-only traversals that treat every variant's
    /// children uniformly (reference collection, "does it contain an X?"
    /// folds); see [`Type::map_children`] to transform them instead.
    #[must_use]
    pub fn children(&self) -> Vec<&Type> {
        match self {
            Type::Arrow(a, b) | Type::Product(a, b) | Type::Sum(a, b) => vec![a, b],
            Type::Forall(_, body) | Type::Mu(_, body) | Type::Ptr(body) | Type::Ref(body) => {
                vec![body]
            }
            Type::Eq(ty_arg, _, _) => vec![ty_arg],
            Type::App(_, args) => args.iter().collect(),
            Type::Adt(_, type_args, variants) => type_args
                .iter()
                .chain(variants.iter().map(|(_, payload)| payload))
                .collect(),
            Type::Bool
            | Type::Nat
            | Type::Int
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::TyVar(_)
            | Type::Error => Vec::new(),
        }
    }

    /// Rebuild `self` with each direct child type replaced by `f(child)`.
    ///
    /// The variant tag, names, bound-variable names, and `Eq` witness terms
    /// are preserved; only the child `Type`s are transformed. Terminal
    /// variants (which have no child types) return a clone. This is the
    /// structural-recursion *default* for a transforming traversal: a walker
    /// overrides only the arms with non-uniform behavior (e.g. `TyVar`
    /// resolution, `App` cycle detection) and delegates the rest via
    /// `_ => ty.map_children(...)`. Adding a new `Type` variant then extends
    /// every such walker automatically with the correct structural default.
    #[must_use]
    pub fn map_children(&self, mut f: impl FnMut(&Type) -> Type) -> Type {
        match self {
            Type::Arrow(a, b) => Type::arrow(f(a), f(b)),
            Type::Product(a, b) => Type::product(f(a), f(b)),
            Type::Sum(a, b) => Type::sum(f(a), f(b)),
            Type::Forall(v, body) => Type::forall(v.clone(), f(body)),
            Type::Mu(v, body) => Type::mu(v.clone(), f(body)),
            Type::Ptr(inner) => Type::ptr(f(inner)),
            Type::Ref(inner) => Type::ref_ty(f(inner)),
            Type::Eq(ty_arg, t1, t2) => Type::eq(f(ty_arg), (**t1).clone(), (**t2).clone()),
            Type::App(name, args) => Type::app(name.clone(), args.iter().map(&mut f).collect()),
            Type::Adt(name, type_args, variants) => Type::adt(
                name.clone(),
                type_args.iter().map(&mut f).collect(),
                variants
                    .iter()
                    .map(|(vname, payload)| (vname.clone(), f(payload)))
                    .collect(),
            ),
            Type::Bool
            | Type::Nat
            | Type::Int
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::TyVar(_)
            | Type::Error => self.clone(),
        }
    }

    /// Reconstruct a binary type (Arrow, Product, or Sum) with new children.
    ///
    /// Panics if `template` is not Arrow, Product, or Sum.
    #[must_use]
    pub fn reconstruct_binary(template: &Type, a: Type, b: Type) -> Type {
        match template {
            Type::Arrow(..) => Type::arrow(a, b),
            Type::Product(..) => Type::product(a, b),
            Type::Sum(..) => Type::sum(a, b),
            _ => unreachable!("reconstruct_binary called on non-binary type"),
        }
    }

    /// Reconstruct a binding type (Forall or Mu) with a new body.
    ///
    /// Panics if `template` is not Forall or Mu.
    pub fn reconstruct_binding(template: &Type, var: impl Into<String>, body: Type) -> Type {
        match template {
            Type::Forall(..) => Type::forall(var, body),
            Type::Mu(..) => Type::mu(var, body),
            _ => unreachable!("reconstruct_binding called on non-binding type"),
        }
    }

    /// Reconstruct a wrapper type (Ptr or Ref) with a new inner type.
    ///
    /// Panics if `template` is not Ptr or Ref.
    #[must_use]
    pub fn reconstruct_wrapper(template: &Type, inner: Type) -> Type {
        match template {
            Type::Ptr(_) => Type::ptr(inner),
            Type::Ref(_) => Type::ref_ty(inner),
            _ => unreachable!("reconstruct_wrapper called on non-wrapper type"),
        }
    }

    /// Count the total number of nodes in this type tree.
    ///
    /// Folds over [`Type::children`], so a new variant is counted by its
    /// structural default rather than by an arm that has to be remembered
    /// (`Eq` counts its type argument only, never its witness terms).
    #[must_use]
    pub fn node_count(&self) -> usize {
        1 + self
            .children()
            .iter()
            .map(|c| c.node_count())
            .sum::<usize>()
    }

    /// Compute the maximum depth of this type tree.
    #[must_use]
    pub fn depth(&self) -> usize {
        1 + self.children().iter().map(|c| c.depth()).max().unwrap_or(0)
    }
}
