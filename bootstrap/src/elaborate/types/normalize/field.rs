//! Field normalization and type argument canonicalization.
//!
//! This module handles normalizing fields within ADT constructors and
//! canonicalizing type arguments for consistent representation.

use std::collections::{HashMap, HashSet};

use crate::elaborate::Elaborator;
use tungsten_core::Type;

impl<'a> Elaborator<'a> {
    /// Canonicalize a type argument without expanding ADTs.
    ///
    /// This ensures consistent representation of type args:
    /// - `App("X", [])` becomes `TyVar("X")` for consistency
    /// - Complex types are recursively canonicalized
    /// - ADTs are NOT expanded (unlike normalize_for_comparison_impl)
    pub(super) fn canonicalize_type_arg(&self, ty: &Type) -> Type {
        match ty {
            // 0-arity App -> TyVar for consistency
            Type::App(name, args) if args.is_empty() => Type::TyVar(name.clone()),
            // Non-empty App: canonicalize args only
            Type::App(name, args) => {
                let can_args: Vec<Type> =
                    args.iter().map(|a| self.canonicalize_type_arg(a)).collect();
                Type::app(name, can_args)
            }
            // Compound types: recurse
            Type::Product(l, r) => {
                Type::product(self.canonicalize_type_arg(l), self.canonicalize_type_arg(r))
            }
            Type::Sum(l, r) => {
                Type::sum(self.canonicalize_type_arg(l), self.canonicalize_type_arg(r))
            }
            Type::Arrow(p, r) => {
                Type::arrow(self.canonicalize_type_arg(p), self.canonicalize_type_arg(r))
            }
            Type::Mu(v, body) => Type::mu(v, self.canonicalize_type_arg(body)),
            Type::Forall(v, body) => Type::forall(v, self.canonicalize_type_arg(body)),
            Type::Ptr(inner) => Type::ptr(self.canonicalize_type_arg(inner)),
            Type::Ref(inner) => Type::ref_ty(self.canonicalize_type_arg(inner)),
            // Base types and TyVar: return as-is
            _ => ty.clone(),
        }
    }

    /// Normalize a field type within an ADT constructor, handling self-references.
    ///
    /// IMPORTANT: We do NOT expand other ADTs inside this ADT's encoding.
    /// This ensures consistency with cached encodings which also keep cross-references
    /// as `App("TypeName", [])` rather than expanding them.
    pub(super) fn normalize_field_for_adt(
        &self,
        field_ty: &Type,
        adt_name: &str,
        subst: &HashMap<&str, &Type>,
        is_recursive: bool,
        mu_var: &str,
        in_progress: &mut HashSet<String>,
    ) -> Type {
        match field_ty {
            Type::TyVar(v) => self.normalize_field_tyvar(
                v,
                field_ty,
                adt_name,
                subst,
                is_recursive,
                mu_var,
                in_progress,
            ),
            Type::App(name, args) => self.normalize_field_app(
                name,
                args,
                field_ty,
                adt_name,
                subst,
                is_recursive,
                mu_var,
                in_progress,
            ),
            Type::Product(l, r) => Type::product(
                self.normalize_field_for_adt(l, adt_name, subst, is_recursive, mu_var, in_progress),
                self.normalize_field_for_adt(r, adt_name, subst, is_recursive, mu_var, in_progress),
            ),
            Type::Sum(l, r) => Type::sum(
                self.normalize_field_for_adt(l, adt_name, subst, is_recursive, mu_var, in_progress),
                self.normalize_field_for_adt(r, adt_name, subst, is_recursive, mu_var, in_progress),
            ),
            Type::Arrow(p, r) => Type::arrow(
                self.normalize_field_for_adt(p, adt_name, subst, is_recursive, mu_var, in_progress),
                self.normalize_field_for_adt(r, adt_name, subst, is_recursive, mu_var, in_progress),
            ),
            Type::Mu(v, body) => Type::mu(
                v,
                self.normalize_field_for_adt(
                    body,
                    adt_name,
                    subst,
                    is_recursive,
                    mu_var,
                    in_progress,
                ),
            ),
            Type::Forall(v, body) => Type::forall(
                v,
                self.normalize_field_for_adt(
                    body,
                    adt_name,
                    subst,
                    is_recursive,
                    mu_var,
                    in_progress,
                ),
            ),
            Type::Ptr(inner) => Type::ptr(self.normalize_field_for_adt(
                inner,
                adt_name,
                subst,
                is_recursive,
                mu_var,
                in_progress,
            )),
            Type::Ref(inner) => Type::ref_ty(self.normalize_field_for_adt(
                inner,
                adt_name,
                subst,
                is_recursive,
                mu_var,
                in_progress,
            )),
            Type::Eq(t, a, b) => Type::eq(
                self.normalize_field_for_adt(t, adt_name, subst, is_recursive, mu_var, in_progress),
                (**a).clone(),
                (**b).clone(),
            ),
            // Flat ADT (ADR 2.2.26) - recursively normalize variants
            Type::Adt(name, type_args, variants) => Type::Adt(
                name.clone(),
                type_args
                    .iter()
                    .map(|t| {
                        self.normalize_field_for_adt(
                            t,
                            adt_name,
                            subst,
                            is_recursive,
                            mu_var,
                            in_progress,
                        )
                    })
                    .collect(),
                variants
                    .iter()
                    .map(|(ctor, payload)| {
                        (
                            ctor.clone(),
                            self.normalize_field_for_adt(
                                payload,
                                adt_name,
                                subst,
                                is_recursive,
                                mu_var,
                                in_progress,
                            ),
                        )
                    })
                    .collect(),
            ),
            // Base types - return as-is
            Type::Unit | Type::Void | Type::Bool | Type::Nat | Type::String | Type::Prop => {
                field_ty.clone()
            }
        }
    }

    /// Normalize a TyVar field - substitute if we have a binding, or normalize if external.
    fn normalize_field_tyvar(
        &self,
        v: &str,
        field_ty: &Type,
        adt_name: &str,
        subst: &HashMap<&str, &Type>,
        is_recursive: bool,
        mu_var: &str,
        in_progress: &mut HashSet<String>,
    ) -> Type {
        if let Some(&replacement) = subst.get(v) {
            // Recursively process the substituted type (but don't expand ADTs)
            self.normalize_field_for_adt(
                replacement,
                adt_name,
                subst,
                is_recursive,
                mu_var,
                in_progress,
            )
        } else if is_recursive && v == adt_name {
            // Self-reference in non-App form (rare but possible)
            Type::TyVar(mu_var.to_string())
        } else {
            // Type variable referring to another type - normalize it!
            // This ensures that nested type references like TypeExpr inside List<TypeExpr>
            // get the same Mu expansion as the "found" type from inference.
            self.normalize_for_comparison_impl(field_ty, in_progress)
        }
    }

    /// Normalize an App field - check for self-reference or normalize external types.
    fn normalize_field_app(
        &self,
        name: &str,
        args: &[Type],
        field_ty: &Type,
        adt_name: &str,
        subst: &HashMap<&str, &Type>,
        is_recursive: bool,
        mu_var: &str,
        in_progress: &mut HashSet<String>,
    ) -> Type {
        if is_recursive && name == adt_name {
            // Self-reference: replace with μ-variable
            Type::TyVar(mu_var.to_string())
        } else if args.is_empty() {
            // 0-arity App referring to another type - normalize it!
            self.normalize_for_comparison_impl(field_ty, in_progress)
        } else {
            // Not a self-reference with args - normalize the args
            let normalized_args: Vec<Type> = args
                .iter()
                .map(|a| {
                    self.normalize_field_for_adt(
                        a,
                        adt_name,
                        subst,
                        is_recursive,
                        mu_var,
                        in_progress,
                    )
                })
                .collect();
            Type::app(name, normalized_args)
        }
    }
}
