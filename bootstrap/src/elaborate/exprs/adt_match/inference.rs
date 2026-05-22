//! Phase 5: Type Inference
//!
//! Determines the result type of a match expression.
//!
//! ## The Generic Pattern Inference Bug (ADR 30.1.26)
//!
//! The `infer_ctor_arm_type` function must use two-phase substitution when
//! computing field types for pattern variables:
//!
//! 1. **Phase 1**: Substitute type parameters (e.g., `T` → `String` for `List<String>`)
//! 2. **Phase 2**: Substitute μ-type recursive references (e.g., `α_List` → full μ-type)
//!
//! The original bug was only doing Phase 2, which left type parameters unsubstituted.
//! This caused errors like:
//!
//! ```text
//! expected `String`, found `Option<(String × α_List)>`
//! ```
//!
//! The fix is to use `instantiate_constructor_fields` which performs both phases.

use crate::ast::{self, Pattern};
use crate::span::{Span, Spanned};
use tungsten_core::Type;

use super::context::ClassifiedArms;
use crate::elaborate::env::{self as elab_env, ModulePath, PathResolutionError};
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

/// Context for match type inference: the unfolded sum type,
/// constructors, and ADT identity needed to infer arm types.
pub(super) struct MatchTypeCtx<'a> {
    pub sum_type: &'a Type,
    pub constructors: &'a [elab_env::Constructor],
    pub scrutinee_ty: &'a Type,
    pub type_params: &'a [String],
}

impl<'a> Elaborator<'a> {
    /// Infer the result type of a match expression.
    pub(super) fn infer_match_result_type(
        &mut self,
        expected: Option<&Type>,
        classified: &ClassifiedArms,
        ctx: &MatchTypeCtx<'_>,
    ) -> ElabResult<Type> {
        if let Some(expected) = expected {
            return Ok(expected.clone());
        }

        // Try constructor arms first - get first arm from the Vec
        if let Some(arms_vec) = classified.ctor_arms.get(&0) {
            if let Some(arm) = arms_vec.first() {
                return self.infer_ctor_arm_type(
                    arm,
                    ctx.sum_type,
                    ctx.constructors,
                    ctx.scrutinee_ty,
                    ctx.type_params,
                );
            }
        }

        if let Some(first_arms_vec) = classified.ctor_arms.values().next() {
            if let Some(first_arm) = first_arms_vec.first() {
                return self.infer_ctor_arm_type(
                    first_arm,
                    ctx.sum_type,
                    ctx.constructors,
                    ctx.scrutinee_ty,
                    ctx.type_params,
                );
            }
        }

        // Fall back to catch-all
        if let Some(catch_all) = classified.catch_all {
            self.env.push_scope();
            let (_, ty) = self.infer(&catch_all.body)?;
            self.env.pop_scope();
            return Ok(ty);
        }

        Err(ElabError::new(
            Span::new(0, 0),
            ElabErrorKind::Other("internal error: no arms to infer type from".to_string()),
        ))
    }

    /// Infer the type of a constructor arm.
    ///
    /// This is a critical function for the generic pattern inference fix (ADR 30.1.26).
    /// It must use `instantiate_constructor_fields` to perform two-phase substitution:
    ///
    /// 1. Substitute type parameters (T → concrete type)
    /// 2. Substitute μ-type references (α_List → full μ-type)
    ///
    /// Without Phase 1, pattern variables get incorrect types with leaked μ-variables.
    pub(super) fn infer_ctor_arm_type(
        &mut self,
        arm: &ast::MatchArm,
        sum_type: &Type,
        constructors: &[elab_env::Constructor],
        adt_type: &Type,
        type_params: &[String], // Type parameters of the ADT (for generic substitution)
    ) -> ElabResult<Type> {
        let Pattern::Constructor(ref path, ref sub_patterns, _) = arm.pattern else {
            return Err(ElabError::new(
                arm.pattern.span(),
                ElabErrorKind::Other("expected constructor pattern".to_string()),
            ));
        };

        // Look up constructor info using path resolution
        let name = path.item_name();

        // Check module visibility for qualified paths
        if !path.is_simple() {
            let module_path = ModulePath::new(
                path.module_segments()
                    .iter()
                    .map(|s| s.name.clone())
                    .collect(),
            );
            if !self
                .env
                .is_module_accessible(&module_path, &self.current_module, true)
            {
                return Err(ElabError::private_module(
                    path.span,
                    module_path.to_string(),
                    self.current_module.to_string(),
                ));
            }
        }

        let info = match self
            .env
            .resolve_constructor_path(path, &self.current_module)
        {
            Ok(Some(info)) => info.clone(),
            Ok(None) => {
                return Err(self.undefined_constructor_error(name.span, &name.name));
            }
            Err(PathResolutionError::ModuleNotFound(module)) => {
                return Err(ElabError::module_not_found(path.span, module.to_string()));
            }
            Err(PathResolutionError::ItemNotFound { module, item }) => {
                return Err(ElabError::item_not_in_module(
                    path.span,
                    module.to_string(),
                    item,
                ));
            }
        };

        let constructor = &constructors[info.index];

        // KEY FIX (ADR 30.1.26): Use instantiate_constructor_fields for two-phase substitution
        // This was the bug - the old code only did Phase 2 (substitute_recursive_refs).
        // Use explicit ADT name for non-recursive types like Option
        let field_types = self.instantiate_constructor_fields_with_name(
            &constructor.fields,
            type_params,
            adt_type,
            &info.type_name,
        );

        // Get the type at this constructor's position in the sum (for validation)
        let _ctor_ty = self.get_sum_component(sum_type, info.index, constructors.len())?;

        self.env.push_scope();

        // Bind pattern variables with the CORRECTLY SUBSTITUTED field types
        for (pat, ty) in sub_patterns.iter().zip(field_types.iter()) {
            if let Pattern::Var(ref var) = pat {
                self.env
                    .bind_local(var.name.clone(), ty.clone(), self.depth);
                self.depth += 1;
            }
        }

        let (_, ty) = self.infer(&arm.body)?;

        // Pop bindings
        for _ in sub_patterns.iter().filter(|p| matches!(p, Pattern::Var(_))) {
            self.depth -= 1;
        }
        self.env.pop_scope();

        Ok(ty)
    }

    /// Get the type of a component at index in a right-nested sum or flat ADT.
    ///
    /// ## Representation Policy (ADR 2.2.26)
    ///
    /// - n = 1: Single constructor, whole type is the payload
    /// - n = 2: Binary sum, navigate nested Sum structure
    /// - n >= 3: Flat ADT, directly index into variants
    pub(in crate::elaborate::exprs) fn get_sum_component(
        &self,
        sum_type: &Type,
        index: usize,
        num_ctors: usize,
    ) -> ElabResult<Type> {
        // n = 1: Single constructor - whole type is payload
        if num_ctors == 1 {
            return Ok(sum_type.clone());
        }

        // n >= 3: Flat ADT - direct variant lookup
        if let Type::Adt(_, _, variants) = sum_type {
            return Self::get_adt_variant_type(variants, index);
        }

        // n = 2: Binary sum - navigate right-nested structure
        self.get_binary_sum_component(sum_type, index, num_ctors)
    }

    /// Extract variant type from flat ADT by index.
    fn get_adt_variant_type(variants: &[(String, Type)], index: usize) -> ElabResult<Type> {
        variants
            .get(index)
            .map(|(_, payload)| payload.clone())
            .ok_or_else(|| {
                ElabError::new(
                    Span::new(0, 0),
                    ElabErrorKind::Other(format!(
                        "variant index {} out of bounds for ADT with {} variants",
                        index,
                        variants.len()
                    )),
                )
            })
    }

    /// Navigate right-nested binary sum to extract component at index.
    ///
    /// For a sum `A + (B + C)` with index 1, returns type `B`.
    fn get_binary_sum_component(
        &self,
        sum_type: &Type,
        index: usize,
        num_ctors: usize,
    ) -> ElabResult<Type> {
        // Navigate to the correct position in the right-nested sum
        let current = self.navigate_sum_to_index(sum_type, index)?;

        // Extract the appropriate component based on position
        self.extract_sum_component_at_position(current, index, num_ctors)
    }

    /// Navigate through right-nested sum structure to reach target index.
    ///
    /// Returns the type at the position where we should extract from.
    fn navigate_sum_to_index<'t>(&self, sum_type: &'t Type, index: usize) -> ElabResult<&'t Type> {
        let mut current = Self::unwrap_mu(sum_type);

        for _ in 0..index {
            current = self.step_right_in_sum(current)?;
        }

        Ok(current)
    }

    /// Take one step right in a sum type (unwrapping Mu if needed).
    fn step_right_in_sum<'t>(&self, ty: &'t Type) -> ElabResult<&'t Type> {
        let unwrapped = if let Type::Mu(_, body) = ty {
            body.as_ref()
        } else {
            ty
        };
        if let Type::Sum(_, right) = unwrapped {
            Ok(right.as_ref())
        } else {
            Err(Self::expected_sum_error())
        }
    }

    /// Extract the left or right component from current position in sum.
    fn extract_sum_component_at_position(
        &self,
        current: &Type,
        index: usize,
        num_ctors: usize,
    ) -> ElabResult<Type> {
        let unwrapped = Self::unwrap_mu(current);

        if index < num_ctors - 1 {
            // Not the last constructor - extract left branch
            Self::extract_left_from_sum(unwrapped)
        } else {
            // Last constructor - extract right branch (or whole type if not a sum)
            Self::extract_right_from_sum(unwrapped)
        }
    }

    /// Extract the left component from a sum type.
    fn extract_left_from_sum(ty: &Type) -> ElabResult<Type> {
        match ty {
            Type::Sum(left, _) => Ok((**left).clone()),
            _ => Err(Self::expected_sum_error()),
        }
    }

    /// Extract the right component from a sum type, or return the type itself.
    fn extract_right_from_sum(ty: &Type) -> ElabResult<Type> {
        match ty {
            Type::Sum(_, right) => Ok((**right).clone()),
            other => Ok(other.clone()), // Last position may not be a sum
        }
    }

    /// Unwrap a Mu type to get its body, or return the type unchanged.
    fn unwrap_mu(ty: &Type) -> &Type {
        match ty {
            Type::Mu(_, body) => body.as_ref(),
            other => other,
        }
    }

    /// Create a standard "expected sum type" error.
    fn expected_sum_error() -> ElabError {
        ElabError::new(
            Span::new(0, 0),
            ElabErrorKind::Other("expected sum type".to_string()),
        )
    }
}
