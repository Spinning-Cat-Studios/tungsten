//! Core implementation of type normalization.
//!
//! This module contains the main `normalize_for_comparison_impl` function
//! and its helper functions for handling different type variants.

use std::collections::HashSet;

use crate::elaborate::env::TypeDefKind;
use crate::elaborate::Elaborator;
use tungsten_core::Type;

impl<'a> Elaborator<'a> {
    /// Internal implementation of type normalization with cycle detection.
    ///
    /// The `in_progress` set tracks type names currently being expanded to detect
    /// and break cycles in mutually recursive type definitions.
    pub(super) fn normalize_for_comparison_impl(
        &self,
        ty: &Type,
        in_progress: &mut HashSet<String>,
    ) -> Type {
        match ty {
            Type::TyVar(name) => self.normalize_tyvar(name, ty, in_progress),
            Type::App(name, args) => self.normalize_app(name, args, ty, in_progress),
            Type::Product(l, r) => self.normalize_product(l, r, in_progress),
            Type::Sum(l, r) => self.normalize_sum(l, r, in_progress),
            Type::Arrow(p, r) => self.normalize_arrow(p, r, in_progress),
            Type::Mu(_v, _body) => {
                // DON'T recursively normalize inside Mu bodies
                // This matches cached encoding behavior where cross-references remain as App
                // If we recursively normalized, we'd expand App("OtherType", []) references
                // which would cause mismatches with cached encodings
                ty.clone()
            }
            Type::Forall(v, body) => Type::forall(
                v.clone(),
                self.normalize_for_comparison_impl(body, in_progress),
            ),
            Type::Ptr(inner) => Type::ptr(self.normalize_for_comparison_impl(inner, in_progress)),
            Type::Ref(inner) => {
                Type::ref_ty(self.normalize_for_comparison_impl(inner, in_progress))
            }
            Type::Eq(ty_arg, a, b) => Type::eq(
                self.normalize_for_comparison_impl(ty_arg, in_progress),
                (**a).clone(),
                (**b).clone(),
            ),
            // Base types and type variables don't need normalization
            _ => ty.clone(),
        }
    }

    /// Normalize a TyVar - check if it's a defined type (record/ADT/alias).
    fn normalize_tyvar(&self, name: &str, ty: &Type, in_progress: &mut HashSet<String>) -> Type {
        // Cycle detection: if we're already expanding this type, return unexpanded
        if in_progress.contains(name) {
            return ty.clone();
        }

        // Look up the type definition and get its encoding
        let Some(type_def) = self.env.lookup_type(name) else {
            // Not a defined type - it's a true type variable, return as-is
            return ty.clone();
        };

        // For records, keep as nominal type (don't use cache, don't expand)
        // This ensures consistency with how constructors are encoded
        if matches!(&type_def.kind, TypeDefKind::Record(_)) {
            return ty.clone();
        }

        // Check for cached encoding (only for non-record types)
        if let Some(ref cached) = type_def.encoded_type {
            // Canonicalize the cached encoding to ensure consistent representation
            // This converts any App("X", []) to TyVar("X") inside Mu bodies
            return self.canonicalize_type_arg(cached);
        }

        // Mark this type as being expanded
        in_progress.insert(name.to_string());

        let result = match &type_def.kind {
            TypeDefKind::ADT(constructors) => {
                // For ADTs, use proper μ-type encoding for recursive types
                self.encode_adt_for_normalization(
                    name,
                    constructors,
                    &[],
                    &type_def.params,
                    in_progress,
                )
            }
            TypeDefKind::Record(_) => {
                // This branch won't be reached due to early return above
                unreachable!()
            }
            TypeDefKind::Alias(aliased) => self.normalize_for_comparison_impl(aliased, in_progress),
            TypeDefKind::Stub => ty.clone(),
        };

        in_progress.remove(name);
        result
    }

    /// Normalize a Type::App for parameterized types.
    fn normalize_app(
        &self,
        name: &str,
        args: &[Type],
        ty: &Type,
        in_progress: &mut HashSet<String>,
    ) -> Type {
        // Cycle detection: if we're already expanding this type, return unexpanded
        // For 0-arity types, return TyVar for consistency with TyVar case
        if in_progress.contains(name) {
            return if args.is_empty() {
                Type::TyVar(name.to_string())
            } else {
                ty.clone()
            };
        }

        // Look up the type definition and get its encoding
        let Some(type_def) = self.env.lookup_type(name) else {
            return ty.clone();
        };

        // For records, keep as nominal type (don't expand)
        if matches!(&type_def.kind, TypeDefKind::Record(_)) {
            return ty.clone();
        }

        // Check for cached encoding (only for non-parameterized, non-record types)
        if args.is_empty() {
            if let Some(ref cached) = type_def.encoded_type {
                // Canonicalize the cached encoding to ensure consistent representation
                // This converts any App("X", []) to TyVar("X") inside Mu bodies
                return self.canonicalize_type_arg(cached);
            }
        }

        // Mark this type as being expanded
        in_progress.insert(name.to_string());

        let result = match &type_def.kind {
            TypeDefKind::ADT(constructors) => {
                // For ADTs, use proper μ-type encoding for recursive types
                self.encode_adt_for_normalization(
                    name,
                    constructors,
                    args,
                    &type_def.params,
                    in_progress,
                )
            }
            TypeDefKind::Record(_) => {
                // This branch won't be reached due to early return above
                unreachable!()
            }
            TypeDefKind::Alias(aliased) => {
                // Substitute type parameters for parameterized aliases
                // Canonicalize args first to ensure consistent representation
                let mut result = aliased.clone();
                for (param, arg) in type_def.params.iter().zip(args.iter()) {
                    let canonical_arg = self.canonicalize_type_arg(arg);
                    result = result.substitute(param, &canonical_arg);
                }
                self.normalize_for_comparison_impl(&result, in_progress)
            }
            TypeDefKind::Stub => {
                // Stub types are incomplete - return as-is
                ty.clone()
            }
        };

        // Done expanding this type
        in_progress.remove(name);
        result
    }

    /// Normalize a Product type by recursively normalizing both components.
    #[inline]
    fn normalize_product(
        &self,
        left: &Type,
        right: &Type,
        in_progress: &mut HashSet<String>,
    ) -> Type {
        Type::product(
            self.normalize_for_comparison_impl(left, in_progress),
            self.normalize_for_comparison_impl(right, in_progress),
        )
    }

    /// Normalize a Sum type by recursively normalizing both components.
    #[inline]
    fn normalize_sum(&self, left: &Type, right: &Type, in_progress: &mut HashSet<String>) -> Type {
        Type::sum(
            self.normalize_for_comparison_impl(left, in_progress),
            self.normalize_for_comparison_impl(right, in_progress),
        )
    }

    /// Normalize an Arrow type by recursively normalizing both components.
    #[inline]
    fn normalize_arrow(&self, param: &Type, ret: &Type, in_progress: &mut HashSet<String>) -> Type {
        Type::arrow(
            self.normalize_for_comparison_impl(param, in_progress),
            self.normalize_for_comparison_impl(ret, in_progress),
        )
    }
}
