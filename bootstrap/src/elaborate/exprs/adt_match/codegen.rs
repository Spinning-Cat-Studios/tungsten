//! Phase 6: Code Generation
//!
//! Generates case expressions for ADT pattern matching.
//!
//! ## Representation Policy (ADR 2.2.26)
//!
//! - n = 1: No case needed, direct binding
//! - n = 2: Binary sum → nested Term::case (existing)
//! - n >= 3: Flat ADT → Term::adt_match with switch dispatch
//!
//! ## Nested Pattern Handling (ADR 3.2.26)
//!
//! When multiple arms match the same outer constructor with different inner patterns:
//! e.g., `Some(TokBang()) => ..., Some(TokMinus()) => ..., _ => ...`
//!
//! The codegen groups these into a single arm for `Some` that contains an inner match
//! on the payload. The catch-all propagates to the inner match.

use std::collections::HashMap;
use std::env;

use crate::ast;
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::env as elab_env;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

/// Check if debug tracing is enabled for type unfolding.
/// Set TUNGSTEN_DEBUG_UNFOLD=1 to enable.
fn debug_unfold_enabled() -> bool {
    env::var("TUNGSTEN_DEBUG_UNFOLD")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Check if debug tracing is enabled for match codegen.
/// Set TUNGSTEN_DEBUG_MATCH=1 to enable.
fn debug_match_enabled() -> bool {
    env::var("TUNGSTEN_DEBUG_MATCH")
        .map(|v| v == "1")
        .unwrap_or(false)
}

impl<'a> Elaborator<'a> {
    /// Get a single arm for a constructor, handling both single-arm and multi-arm cases.
    ///
    /// When multiple arms match the same outer constructor (e.g., `Some(A)`, `Some(B)`),
    /// this function returns the FIRST arm. The multi-arm case needs special handling
    /// in `build_nested_match_for_arms` which builds an inner match.
    fn get_arm_for_ctor<'b>(
        ctor_arms: &HashMap<usize, Vec<&'b ast::MatchArm>>,
        ctor_index: usize,
        catch_all_arm: Option<&'b ast::MatchArm>,
    ) -> Option<&'b ast::MatchArm> {
        ctor_arms
            .get(&ctor_index)
            .and_then(|arms| arms.first().copied())
            .or(catch_all_arm)
    }

    /// Check if constructor has multiple arms (needs inner match).
    fn has_multiple_arms(
        ctor_arms: &HashMap<usize, Vec<&ast::MatchArm>>,
        ctor_index: usize,
    ) -> bool {
        ctor_arms
            .get(&ctor_index)
            .map(|v| v.len() > 1)
            .unwrap_or(false)
    }

    /// Build nested case analysis for ADT matching.
    ///
    /// For n constructors with right-nested sum: `A + (B + (C + D))`
    /// - `case scrutinee of inl(a) => arm0 | inr(rest) => recurse`
    ///
    /// If `catch_all_arm` is Some, use it for any constructor indices not in `ctor_arms`.
    pub(super) fn build_adt_match(
        &mut self,
        scrutinee: Term,
        sum_type: &Type,
        ctor_arms: &HashMap<usize, Vec<&ast::MatchArm>>,
        catch_all_arm: Option<&ast::MatchArm>,
        constructors: &[elab_env::Constructor],
        ctor_index: usize,
        result_ty: &Type,
        adt_type: &Type,
        type_params: &[String],
        adt_name: &str,
    ) -> ElabResult<Term> {
        let num_ctors = constructors.len();

        // Handle edge cases: single constructor or last constructor
        if num_ctors == 1 {
            return self.build_single_ctor_match(
                scrutinee,
                sum_type,
                ctor_arms,
                catch_all_arm,
                &constructors[0],
                adt_type,
                type_params,
                adt_name,
            );
        }

        if ctor_index == num_ctors - 1 {
            return self.build_last_ctor_match(
                scrutinee,
                sum_type,
                ctor_arms,
                catch_all_arm,
                &constructors[ctor_index],
                ctor_index,
                adt_type,
                type_params,
                adt_name,
            );
        }

        // Extract left and right types from the sum
        let (left_ty, right_ty) = self.extract_sum_types(
            sum_type,
            adt_name,
            adt_type,
            ctor_index,
            num_ctors,
            constructors,
        )?;

        // Debug output at start of match
        self.debug_adt_match_start(ctor_arms, catch_all_arm, constructors, ctor_index, adt_name);

        // Elaborate the left arm (current constructor)
        let (left_var, left_body) = self.elaborate_ctor_arm(
            ctor_arms,
            catch_all_arm,
            left_ty,
            &constructors[ctor_index],
            ctor_index,
            adt_type,
            type_params,
            adt_name,
        )?;

        // Debug output for specific constructors
        self.debug_ctor_body(&left_var, &left_body, ctor_index, adt_name);

        // Recursively build the right side
        let (right_var, right_body) = self.build_right_branch(
            right_ty,
            ctor_arms,
            catch_all_arm,
            constructors,
            ctor_index,
            result_ty,
            adt_type,
            type_params,
            adt_name,
        )?;

        Ok(Term::case(
            scrutinee, left_var, left_body, right_var, right_body,
        ))
    }

    /// Handle ADT with exactly one constructor.
    ///
    /// No case expression needed - just bind the variable to the scrutinee.
    fn build_single_ctor_match(
        &mut self,
        scrutinee: Term,
        payload_ty: &Type,
        ctor_arms: &HashMap<usize, Vec<&ast::MatchArm>>,
        catch_all_arm: Option<&ast::MatchArm>,
        constructor: &elab_env::Constructor,
        adt_type: &Type,
        type_params: &[String],
        adt_name: &str,
    ) -> ElabResult<Term> {
        let (raw_var, body) = self.elaborate_ctor_arm(
            ctor_arms,
            catch_all_arm,
            payload_ty,
            constructor,
            0,
            adt_type,
            type_params,
            adt_name,
        )?;
        Ok(Term::let_in(&raw_var, payload_ty.clone(), scrutinee, body))
    }

    /// Handle the last constructor in a multi-constructor ADT.
    ///
    /// No more right branches to peel - bind variable to scrutinee.
    fn build_last_ctor_match(
        &mut self,
        scrutinee: Term,
        payload_ty: &Type,
        ctor_arms: &HashMap<usize, Vec<&ast::MatchArm>>,
        catch_all_arm: Option<&ast::MatchArm>,
        constructor: &elab_env::Constructor,
        ctor_index: usize,
        adt_type: &Type,
        type_params: &[String],
        adt_name: &str,
    ) -> ElabResult<Term> {
        let (raw_var, body) = self.elaborate_ctor_arm(
            ctor_arms,
            catch_all_arm,
            payload_ty,
            constructor,
            ctor_index,
            adt_type,
            type_params,
            adt_name,
        )?;
        Ok(Term::let_in(&raw_var, payload_ty.clone(), scrutinee, body))
    }

    /// Extract left and right types from a Sum type.
    fn extract_sum_types<'b>(
        &self,
        sum_type: &'b Type,
        adt_name: &str,
        adt_type: &Type,
        ctor_index: usize,
        num_ctors: usize,
        constructors: &[elab_env::Constructor],
    ) -> ElabResult<(&'b Type, &'b Type)> {
        match sum_type {
            Type::Sum(l, r) => Ok((l.as_ref(), r.as_ref())),
            _ => {
                if debug_unfold_enabled() {
                    eprintln!("\n=== E9999 TRIGGER in build_adt_match ===");
                    eprintln!("  adt_name: {}", adt_name);
                    eprintln!("  adt_type: {:?}", adt_type);
                    eprintln!("  sum_type (expected Sum, got): {:?}", sum_type);
                    eprintln!("  ctor_index: {}", ctor_index);
                    eprintln!("  num_ctors: {}", num_ctors);
                    eprintln!(
                        "  constructors: {:?}",
                        constructors.iter().map(|c| &c.name).collect::<Vec<_>>()
                    );
                }
                Err(ElabError::new(
                    Span::new(0, 0),
                    ElabErrorKind::Other(format!(
                        "expected sum type in ADT match for {}, got {:?}",
                        adt_name, sum_type
                    )),
                ))
            }
        }
    }

    /// Elaborate a constructor arm, handling multi-arm cases with nested patterns.
    fn elaborate_ctor_arm(
        &mut self,
        ctor_arms: &HashMap<usize, Vec<&ast::MatchArm>>,
        catch_all_arm: Option<&ast::MatchArm>,
        payload_ty: &Type,
        constructor: &elab_env::Constructor,
        ctor_index: usize,
        adt_type: &Type,
        type_params: &[String],
        adt_name: &str,
    ) -> ElabResult<(String, Term)> {
        if Self::has_multiple_arms(ctor_arms, ctor_index) {
            // Multiple arms with same outer constructor - build inner match
            let arms_vec = ctor_arms.get(&ctor_index).unwrap();
            self.build_nested_match_for_arms(
                arms_vec,
                catch_all_arm,
                payload_ty,
                constructor,
                adt_type,
                type_params,
                adt_name,
            )
        } else {
            let arm = Self::get_arm_for_ctor(ctor_arms, ctor_index, catch_all_arm).unwrap();

            if debug_match_enabled() && adt_name == "TokenKind" && ctor_index >= 82 {
                eprintln!(
                    "[TokenKind ctor_index={}] using arm with pattern: {:?}",
                    ctor_index, arm.pattern
                );
            }

            self.elab_ctor_arm_or_catch_all(
                arm,
                payload_ty,
                constructor,
                adt_type,
                type_params,
                adt_name,
            )
        }
    }

    /// Build the right branch of a case expression recursively.
    fn build_right_branch(
        &mut self,
        right_ty: &Type,
        ctor_arms: &HashMap<usize, Vec<&ast::MatchArm>>,
        catch_all_arm: Option<&ast::MatchArm>,
        constructors: &[elab_env::Constructor],
        ctor_index: usize,
        result_ty: &Type,
        adt_type: &Type,
        type_params: &[String],
        adt_name: &str,
    ) -> ElabResult<(String, Term)> {
        let right_var = format!("__rest{}", ctor_index);

        self.env.push_scope();
        self.env
            .bind_local(right_var.clone(), right_ty.clone(), self.depth);
        self.depth += 1;

        let right_body = self.build_adt_match(
            Term::var(&right_var),
            right_ty,
            ctor_arms,
            catch_all_arm,
            constructors,
            ctor_index + 1,
            result_ty,
            adt_type,
            type_params,
            adt_name,
        )?;

        self.depth -= 1;
        self.env.pop_scope();

        Ok((right_var, right_body))
    }

    /// Debug output at the start of ADT match elaboration.
    fn debug_adt_match_start(
        &self,
        ctor_arms: &HashMap<usize, Vec<&ast::MatchArm>>,
        catch_all_arm: Option<&ast::MatchArm>,
        constructors: &[elab_env::Constructor],
        ctor_index: usize,
        adt_name: &str,
    ) {
        if debug_match_enabled() && ctor_index == 0 {
            eprintln!("\n=== ADT Match Debug for {} ===", adt_name);
            eprintln!("ctor_arms keys: {:?}", ctor_arms.keys().collect::<Vec<_>>());
            eprintln!("catch_all_arm: {:?}", catch_all_arm.is_some());
            for (idx, ctor) in constructors.iter().enumerate() {
                let arm_count = ctor_arms.get(&idx).map(|v| v.len()).unwrap_or(0);
                eprintln!("  [{}] {} -> {} arm(s)", idx, ctor.name, arm_count);
            }
        }
    }

    /// Debug output for specific constructor bodies.
    fn debug_ctor_body(&self, left_var: &str, left_body: &Term, ctor_index: usize, adt_name: &str) {
        if debug_match_enabled() && adt_name == "TokenKind" && ctor_index == 83 {
            eprintln!(
                "[TokenKind TokEof] left_var={}, body={:?}",
                left_var, left_body
            );
        }
    }

    /// Build flat ADT match for types with 3+ constructors (ADR 2.2.26).
    ///
    /// Generates `Term::adt_match` with O(1) switch dispatch instead of
    /// nested O(n) case expressions.
    ///
    /// # Arguments
    /// - `scrutinee`: The term being matched (must be Type::Adt)
    /// - `ctor_arms`: Map of constructor index → specific arms (may have multiple)
    /// - `catch_all_arm`: Optional default arm for unmatched constructors
    /// - `constructors`: All constructors for this ADT
    /// - `result_ty`: Expected result type of the match
    /// - `adt_type`: The ADT type (for recursive references and field type instantiation)
    /// - `type_params`: Type parameters for generic substitution
    /// - `adt_name`: Name of the ADT
    pub(super) fn build_flat_adt_match(
        &mut self,
        scrutinee: Term,
        ctor_arms: &HashMap<usize, Vec<&ast::MatchArm>>,
        catch_all_arm: Option<&ast::MatchArm>,
        constructors: &[elab_env::Constructor],
        _result_ty: &Type,
        adt_type: &Type,
        type_params: &[String],
        adt_name: &str,
    ) -> ElabResult<Term> {
        let num_ctors = constructors.len();

        if debug_match_enabled() {
            eprintln!(
                "\n=== Flat ADT Match for {} ({} constructors) ===",
                adt_name, num_ctors
            );
            eprintln!("ctor_arms keys: {:?}", ctor_arms.keys().collect::<Vec<_>>());
            eprintln!("catch_all_arm: {:?}", catch_all_arm.is_some());
        }

        // Build arms for all constructors
        let mut arms: Vec<(usize, String, Box<Term>)> = Vec::with_capacity(num_ctors);

        for (idx, ctor) in constructors.iter().enumerate() {
            // Get the payload type for this constructor
            let field_types = self.instantiate_constructor_fields_with_name(
                &ctor.fields,
                type_params,
                adt_type,
                adt_name,
            );

            // The payload type is the product of all fields (or Unit if nullary)
            let payload_ty = if field_types.is_empty() {
                Type::Unit
            } else if field_types.len() == 1 {
                field_types[0].clone()
            } else {
                // Left-nested product: ((a, b), c)
                let mut iter = field_types.iter();
                let mut product = iter.next().unwrap().clone();
                for ty in iter {
                    product = Type::product(product, ty.clone());
                }
                product
            };

            // Handle multiple arms for same constructor vs single arm vs catch-all
            let (var, body) = if Self::has_multiple_arms(ctor_arms, idx) {
                let arms_vec = ctor_arms.get(&idx).unwrap();
                self.build_nested_match_for_arms(
                    arms_vec,
                    catch_all_arm,
                    &payload_ty,
                    ctor,
                    adt_type,
                    type_params,
                    adt_name,
                )?
            } else {
                let arm =
                    Self::get_arm_for_ctor(ctor_arms, idx, catch_all_arm).ok_or_else(|| {
                        ElabError::new(Span::new(0, 0), ElabErrorKind::NonExhaustiveMatch)
                            .with_note(format!(
                                "no pattern for constructor {} at index {}",
                                ctor.name, idx
                            ))
                    })?;

                self.elab_ctor_arm_or_catch_all(
                    arm,
                    &payload_ty,
                    ctor,
                    adt_type,
                    type_params,
                    adt_name,
                )?
            };

            if debug_match_enabled() && adt_name == "TokenKind" {
                eprintln!("  [{}] {} -> var={}", idx, ctor.name, var);
            }

            arms.push((idx, var, Box::new(body)));
        }

        // Create the Term::adt_match
        Ok(Term::AdtMatch(Box::new(scrutinee), arms))
    }

    /// Build a nested match for multiple arms that match the same outer constructor
    /// with different inner patterns.
    ///
    /// For example: `Some(TokBang()) => ..., Some(TokMinus()) => ..., _ => ...`
    /// becomes an inner match on TokenKind for the `Some` constructor's payload.
    ///
    /// If the inner patterns are NOT constructor patterns (e.g., variables),
    /// we just use the first arm since the others are unreachable.
    ///
    /// IMPORTANT: Nested matching only works for single-field constructors where
    /// the payload is a sum/ADT type. For multi-field constructors (product types),
    /// we fall back to using the first arm.
    fn build_nested_match_for_arms(
        &mut self,
        arms: &[&ast::MatchArm],
        catch_all_arm: Option<&ast::MatchArm>,
        payload_ty: &Type,
        outer_ctor: &elab_env::Constructor,
        adt_type: &Type,
        type_params: &[String],
        adt_name: &str,
    ) -> ElabResult<(String, Term)> {
        // Check if nested matching is possible and beneficial
        if !self.should_build_nested_match(arms, payload_ty, outer_ctor) {
            // Fall back to using the first arm
            return self.elab_ctor_arm_or_catch_all(
                arms[0],
                payload_ty,
                outer_ctor,
                adt_type,
                type_params,
                adt_name,
            );
        }

        // Extract inner patterns from constructor arms
        let inner_arms = self.extract_inner_arms(arms)?;

        // Build the inner match with proper scoping
        self.build_scoped_inner_match(&inner_arms, catch_all_arm, payload_ty, outer_ctor)
    }

    /// Determine if we should build a nested match for these arms.
    ///
    /// Returns true only if:
    /// 1. At least one arm has a nested constructor pattern
    /// 2. The outer constructor has exactly one field
    /// 3. The payload type is a matchable ADT/sum
    fn should_build_nested_match(
        &self,
        arms: &[&ast::MatchArm],
        payload_ty: &Type,
        outer_ctor: &elab_env::Constructor,
    ) -> bool {
        let has_nested_constructors = Self::arms_have_nested_constructors(arms);
        let is_single_field = outer_ctor.fields.len() == 1;
        let payload_is_matchable = self.try_resolve_adt_from_type(payload_ty);

        has_nested_constructors && is_single_field && payload_is_matchable
    }

    /// Check if any arm has nested constructor patterns.
    fn arms_have_nested_constructors(arms: &[&ast::MatchArm]) -> bool {
        use crate::ast::Pattern;

        arms.iter().any(|arm| {
            if let Pattern::Constructor(_, sub_patterns, _) = &arm.pattern {
                sub_patterns
                    .iter()
                    .any(|p| matches!(p, Pattern::Constructor(..)))
            } else {
                false
            }
        })
    }

    /// Extract inner patterns from constructor arms.
    ///
    /// Each arm's outer constructor pattern is unwrapped to get the inner pattern.
    fn extract_inner_arms(&self, arms: &[&ast::MatchArm]) -> ElabResult<Vec<ast::MatchArm>> {
        use crate::ast::Pattern;

        let mut inner_arms = Vec::with_capacity(arms.len());

        for arm in arms {
            let inner_arm = self.extract_single_inner_arm(arm)?;
            inner_arms.push(inner_arm);
        }

        Ok(inner_arms)
    }

    /// Extract the inner pattern from a single constructor arm.
    fn extract_single_inner_arm(&self, arm: &ast::MatchArm) -> ElabResult<ast::MatchArm> {
        use crate::ast::Pattern;

        match &arm.pattern {
            Pattern::Constructor(_, sub_patterns, _) => {
                if sub_patterns.len() != 1 {
                    return Err(ElabError::new(
                        arm.pattern.span(),
                        ElabErrorKind::Other(
                            "nested patterns with multiple fields not yet supported".to_string(),
                        ),
                    ));
                }

                Ok(ast::MatchArm {
                    pattern: sub_patterns[0].clone(),
                    guard: arm.guard.clone(),
                    body: arm.body.clone(),
                    span: arm.span,
                })
            }
            _ => Err(ElabError::new(
                arm.pattern.span(),
                ElabErrorKind::Other("expected constructor pattern for nested match".to_string()),
            )),
        }
    }

    /// Build inner match with proper scope management.
    fn build_scoped_inner_match(
        &mut self,
        inner_arms: &[ast::MatchArm],
        catch_all_arm: Option<&ast::MatchArm>,
        payload_ty: &Type,
        outer_ctor: &elab_env::Constructor,
    ) -> ElabResult<(String, Term)> {
        let raw_var = format!("__nested_{}", outer_ctor.name);

        // Bind the variable and build inner match in new scope
        self.env.push_scope();
        self.env
            .bind_local(raw_var.clone(), payload_ty.clone(), self.depth);
        self.depth += 1;

        let scrutinee = Term::var(&raw_var);
        let inner_body =
            self.elaborate_match_with_arms(&scrutinee, payload_ty, inner_arms, catch_all_arm)?;

        self.depth -= 1;
        self.env.pop_scope();

        Ok((raw_var, inner_body))
    }

    /// Elaborate a match expression with given arms (used for nested pattern handling).
    fn elaborate_match_with_arms(
        &mut self,
        scrutinee: &Term,
        scrutinee_ty: &Type,
        inner_arms: &[ast::MatchArm],
        outer_catch_all: Option<&ast::MatchArm>,
    ) -> ElabResult<Term> {
        // Combine inner_arms with catch_all if present
        let mut all_arms: Vec<ast::MatchArm> = inner_arms.to_vec();
        if let Some(catch_all) = outer_catch_all {
            all_arms.push(catch_all.clone());
        }

        // Check if this is actually an ADT type - if not, fall back to simple match
        // Try to get ADT info - if it fails, we can't do ADT match
        let is_adt = self.try_resolve_adt_from_type(scrutinee_ty);

        if is_adt {
            // Delegate to elab_adt_match which handles all the phases
            let (term, _ty) = self.elab_adt_match(
                scrutinee.clone(),
                scrutinee_ty.clone(),
                &all_arms,
                None, // no expected type
                Span::new(0, 0),
            )?;
            Ok(term)
        } else {
            // Not an ADT - use the first arm's body directly
            // This handles cases like matching on non-sum types
            if let Some(first_arm) = all_arms.first() {
                let (term, _ty) = self.infer(&first_arm.body)?;
                Ok(term)
            } else {
                Err(ElabError::new(
                    Span::new(0, 0),
                    ElabErrorKind::Other("no arms for nested match".to_string()),
                ))
            }
        }
    }

    /// Try to resolve ADT info from a type, returning true if it's an ADT we can match on.
    fn try_resolve_adt_from_type(&self, ty: &Type) -> bool {
        match ty {
            Type::Adt(name, _, _) => self.env.lookup_type(name).is_some(),
            Type::App(name, _) => {
                // Cross-module type reference - check if it resolves to an ADT
                self.env.lookup_type(name).is_some()
            }
            Type::Mu(_, body) => self.try_resolve_adt_from_type(body),
            Type::Sum(_, _) => true, // Binary sum is also matchable
            _ => false,
        }
    }
}
