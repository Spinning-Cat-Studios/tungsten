//! Nested constructor pattern elaboration (Phase C).
//!
//! Handles:
//! - `elab_nested_ctor_pattern` - elaborate nested constructor patterns
//! - `collect_pattern_bindings` - gather variable bindings from patterns
//! - `wrap_nested_ctor_pattern` - wrap body with pattern destructs
//! - `build_ctor_extraction` - extract constructor from sum type

use crate::ast::{self, Pattern};
use crate::config::MAX_PATTERN_DEPTH;
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use super::helpers::PatternBinding;
use crate::elaborate::env::{self as elab_env, ModulePath, PathResolutionError};
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate a nested constructor pattern by:
    /// 1. First, collect all variable bindings and bind them in the environment
    /// 2. Then, elaborate the body with those bindings in scope
    /// 3. Finally, wrap the body with the appropriate destructs and cases
    ///
    /// Returns the wrapped body term.
    pub(super) fn elab_nested_ctor_pattern(
        &mut self,
        pattern: &Pattern,
        value_var: &str,
        value_ty: &Type,
        body_expr: &ast::Expr,
        depth: usize,
    ) -> ElabResult<Term> {
        // Check depth limit
        if depth > MAX_PATTERN_DEPTH {
            return Err(ElabError::new(
                pattern.span(),
                ElabErrorKind::PatternTooDeep {
                    depth,
                    max: MAX_PATTERN_DEPTH,
                },
            ));
        }

        let Pattern::Constructor(ref ctor_path, ref sub_patterns, _) = pattern else {
            return Err(ElabError::new(
                pattern.span(),
                ElabErrorKind::Other("expected constructor pattern".to_string()),
            ));
        };

        let ctor_name = ctor_path.item_name();

        // Check module visibility for qualified paths
        if !ctor_path.is_simple() {
            let module_path = ModulePath::new(
                ctor_path
                    .module_segments()
                    .iter()
                    .map(|s| s.name.clone())
                    .collect(),
            );
            if !self
                .env
                .is_module_accessible(&module_path, &self.current_module, true)
            {
                return Err(ElabError::private_module(
                    ctor_path.span,
                    module_path.to_string(),
                    self.current_module.to_string(),
                ));
            }
        }

        // Look up constructor info using path resolution
        let ctor_info = match self
            .env
            .resolve_constructor_path(ctor_path, &self.current_module)
        {
            Ok(Some(info)) => info.clone(),
            Ok(None) => {
                return Err(self.undefined_constructor_error(ctor_name.span, &ctor_name.name));
            }
            Err(PathResolutionError::ModuleNotFound(module)) => {
                return Err(ElabError::module_not_found(
                    ctor_path.span,
                    module.to_string(),
                ));
            }
            Err(PathResolutionError::ItemNotFound { module, item }) => {
                return Err(ElabError::item_not_in_module(
                    ctor_path.span,
                    module.to_string(),
                    item,
                ));
            }
        };
        let type_name = ctor_info.type_name.clone();
        let ctor_index = ctor_info.index;
        let ctor_arity = ctor_info.arity;

        // Check arity
        if sub_patterns.len() != ctor_arity {
            return Err(ElabError::new(
                pattern.span(),
                ElabErrorKind::ArityMismatch {
                    expected: ctor_arity,
                    found: sub_patterns.len(),
                },
            ));
        }

        // Look up the type definition to get constructors and field types
        let Some(type_def) = self.env.lookup_type(&type_name) else {
            return Err(ElabError::new(
                pattern.span(),
                ElabErrorKind::Other(format!("type '{}' not found", type_name)),
            ));
        };

        let elab_env::TypeDefKind::ADT(constructors) = &type_def.kind else {
            return Err(ElabError::new(
                pattern.span(),
                ElabErrorKind::Other(format!("'{}' is not an algebraic data type", type_name)),
            ));
        };
        let constructors = constructors.clone();
        let type_params = type_def.params.clone();
        let constructor = constructors[ctor_index].clone();

        // Instantiate constructor field types with proper two-phase substitution
        // (type params first, then μ-type unfolding - see ADR 24.1.26)
        // Use the explicit ADT name version for non-recursive types like Option
        let field_types = self.instantiate_constructor_fields_with_name(
            &constructor.fields,
            &type_params,
            value_ty,
            &type_name,
        );

        // Phase 1: Collect and bind all variables from sub-patterns
        let mut bindings: Vec<PatternBinding> = Vec::new();
        self.collect_pattern_bindings(sub_patterns, &field_types, &mut bindings, depth + 1)?;

        // Bind all collected variables to the environment
        self.env.push_scope();
        for binding in &bindings {
            self.env
                .bind_local(binding.var_name.clone(), binding.var_ty.clone(), self.depth);
            self.depth += 1;
        }

        // Phase 2: Elaborate the body with bindings in scope
        let body_term = self.infer(body_expr)?.0;

        // Phase 3: Wrap the body with pattern destructs
        let wrapped = self.wrap_nested_ctor_pattern(
            body_term,
            &constructor,
            ctor_index,
            &constructors,
            sub_patterns,
            &field_types,
            value_var,
            value_ty,
            depth,
            pattern.span(),
        )?;

        // Pop scope and depth
        for _ in &bindings {
            self.depth -= 1;
        }
        self.env.pop_scope();

        Ok(wrapped)
    }

    /// Collect variable bindings from a list of patterns.
    pub(super) fn collect_pattern_bindings(
        &mut self,
        patterns: &[Pattern],
        field_types: &[Type],
        bindings: &mut Vec<PatternBinding>,
        depth: usize,
    ) -> ElabResult<()> {
        for (pat, ty) in patterns.iter().zip(field_types.iter()) {
            self.collect_binding_from_pattern(pat, ty, bindings, depth)?;
        }
        Ok(())
    }

    /// Collect variable binding from a single pattern.
    fn collect_binding_from_pattern(
        &mut self,
        pattern: &Pattern,
        pattern_ty: &Type,
        bindings: &mut Vec<PatternBinding>,
        depth: usize,
    ) -> ElabResult<()> {
        // Check depth limit
        if depth > MAX_PATTERN_DEPTH {
            return Err(ElabError::new(
                pattern.span(),
                ElabErrorKind::PatternTooDeep {
                    depth,
                    max: MAX_PATTERN_DEPTH,
                },
            ));
        }

        match pattern {
            Pattern::Wildcard(_) => {
                // No binding
            }
            Pattern::Var(ref var) => {
                bindings.push(PatternBinding {
                    var_name: var.name.clone(),
                    var_ty: pattern_ty.clone(),
                });
            }
            Pattern::Constructor(ref ctor_path, ref sub_patterns, _) => {
                let ctor_name = ctor_path.item_name();

                // Check module visibility for qualified paths
                if !ctor_path.is_simple() {
                    let module_path = ModulePath::new(
                        ctor_path
                            .module_segments()
                            .iter()
                            .map(|s| s.name.clone())
                            .collect(),
                    );
                    if !self
                        .env
                        .is_module_accessible(&module_path, &self.current_module, true)
                    {
                        return Err(ElabError::private_module(
                            ctor_path.span,
                            module_path.to_string(),
                            self.current_module.to_string(),
                        ));
                    }
                }

                // Look up constructor using path resolution to get field types
                let ctor_info = match self
                    .env
                    .resolve_constructor_path(ctor_path, &self.current_module)
                {
                    Ok(Some(info)) => info.clone(),
                    Ok(None) => {
                        return Err(
                            self.undefined_constructor_error(ctor_name.span, &ctor_name.name)
                        );
                    }
                    Err(PathResolutionError::ModuleNotFound(module)) => {
                        return Err(ElabError::module_not_found(
                            ctor_path.span,
                            module.to_string(),
                        ));
                    }
                    Err(PathResolutionError::ItemNotFound { module, item }) => {
                        return Err(ElabError::item_not_in_module(
                            ctor_path.span,
                            module.to_string(),
                            item,
                        ));
                    }
                };
                let Some(type_def) = self.env.lookup_type(&ctor_info.type_name).cloned() else {
                    return Err(ElabError::new(
                        pattern.span(),
                        ElabErrorKind::Other(format!("type '{}' not found", ctor_info.type_name)),
                    ));
                };
                let elab_env::TypeDefKind::ADT(ref constructors) = type_def.kind else {
                    return Err(ElabError::new(
                        pattern.span(),
                        ElabErrorKind::Other(format!("'{}' is not an ADT", ctor_info.type_name)),
                    ));
                };
                let constructor = &constructors[ctor_info.index];
                let type_params = &type_def.params;

                // Instantiate constructor field types with proper two-phase substitution
                // (type params first, then μ-type unfolding - see ADR 24.1.26)
                // Use explicit ADT name for non-recursive types like Option
                let field_types = self.instantiate_constructor_fields_with_name(
                    &constructor.fields,
                    type_params,
                    pattern_ty,
                    &ctor_info.type_name,
                );

                // Recursively collect from sub-patterns
                self.collect_pattern_bindings(sub_patterns, &field_types, bindings, depth + 1)?;
            }
            _ => {
                return Err(ElabError::unsupported(pattern.span(), "this pattern kind"));
            }
        }
        Ok(())
    }

    /// Wrap a body term with the destructs and cases for a nested constructor pattern.
    fn wrap_nested_ctor_pattern(
        &mut self,
        body: Term,
        constructor: &elab_env::Constructor,
        ctor_index: usize,
        constructors: &[elab_env::Constructor],
        sub_patterns: &[Pattern],
        field_types: &[Type],
        value_var: &str,
        value_ty: &Type,
        depth: usize,
        pattern_span: Span,
    ) -> ElabResult<Term> {
        let num_ctors = constructors.len();

        // Check if it's a recursive type (μ-type) - need to unfold first
        let is_recursive = self.adt_is_recursive(&constructor.name, constructors);

        // Get the unfolded sum type
        let sum_type = if is_recursive {
            match value_ty {
                Type::Mu(_, body) => (**body).clone(),
                _ => value_ty.clone(),
            }
        } else {
            value_ty.clone()
        };

        // The value we'll match on (possibly unfolded)
        let match_value = if is_recursive {
            Term::unfold(value_ty.clone(), Term::var(value_var))
        } else {
            Term::var(value_var)
        };

        // Get the type at this constructor's position in the sum
        let ctor_ty = self.get_sum_component(&sum_type, ctor_index, num_ctors)?;

        // Create a fresh variable for the matched constructor's payload
        let raw_var = format!("__nest{}_{}", depth, constructor.name);

        // Build the body with sub-pattern bindings
        let body_with_bindings = if sub_patterns.is_empty() {
            body
        } else if sub_patterns.len() == 1 {
            self.wrap_single_subpattern(&sub_patterns[0], &field_types[0], &raw_var, body, depth)?
        } else {
            self.wrap_product_subpatterns(sub_patterns, field_types, &raw_var, body, depth)?
        };

        // Build the case expression that matches this constructor
        if num_ctors == 1 {
            Ok(Term::let_in(
                &raw_var,
                ctor_ty.clone(),
                match_value,
                body_with_bindings,
            ))
        } else {
            self.build_ctor_extraction(
                match_value,
                &sum_type,
                ctor_index,
                num_ctors,
                &raw_var,
                body_with_bindings,
                pattern_span,
            )
        }
    }

    /// Wrap body with binding for a single sub-pattern.
    fn wrap_single_subpattern(
        &mut self,
        pattern: &Pattern,
        pattern_ty: &Type,
        value_var: &str,
        body: Term,
        depth: usize,
    ) -> ElabResult<Term> {
        match pattern {
            Pattern::Wildcard(_) => Ok(body),
            Pattern::Var(ref var) => Ok(Term::let_in(
                &var.name,
                pattern_ty.clone(),
                Term::var(value_var),
                body,
            )),
            Pattern::Constructor(ref ctor_path, ref sub_patterns, _) => {
                // Nested constructor - wrap recursively
                let ctor_name = ctor_path.item_name();

                // Check module visibility for qualified paths
                if !ctor_path.is_simple() {
                    let module_path = ModulePath::new(
                        ctor_path
                            .module_segments()
                            .iter()
                            .map(|s| s.name.clone())
                            .collect(),
                    );
                    if !self
                        .env
                        .is_module_accessible(&module_path, &self.current_module, true)
                    {
                        return Err(ElabError::private_module(
                            ctor_path.span,
                            module_path.to_string(),
                            self.current_module.to_string(),
                        ));
                    }
                }

                // Look up constructor info using path resolution
                let ctor_info = match self
                    .env
                    .resolve_constructor_path(ctor_path, &self.current_module)
                {
                    Ok(Some(info)) => info.clone(),
                    Ok(None) => {
                        return Err(
                            self.undefined_constructor_error(ctor_name.span, &ctor_name.name)
                        );
                    }
                    Err(PathResolutionError::ModuleNotFound(module)) => {
                        return Err(ElabError::module_not_found(
                            ctor_path.span,
                            module.to_string(),
                        ));
                    }
                    Err(PathResolutionError::ItemNotFound { module, item }) => {
                        return Err(ElabError::item_not_in_module(
                            ctor_path.span,
                            module.to_string(),
                            item,
                        ));
                    }
                };
                let Some(type_def) = self.env.lookup_type(&ctor_info.type_name) else {
                    return Err(ElabError::new(
                        pattern.span(),
                        ElabErrorKind::Other(format!("type '{}' not found", ctor_info.type_name)),
                    ));
                };
                let elab_env::TypeDefKind::ADT(constructors) = &type_def.kind else {
                    return Err(ElabError::new(
                        pattern.span(),
                        ElabErrorKind::Other(format!("'{}' is not an ADT", ctor_info.type_name)),
                    ));
                };
                let constructors = constructors.clone();
                let type_params = type_def.params.clone();
                let constructor = constructors[ctor_info.index].clone();
                let adt_name = ctor_info.type_name.clone();
                // Instantiate constructor field types with proper two-phase substitution
                // (type params first, then μ-type unfolding - see ADR 24.1.26, ADR 30.1.26)
                // Use explicit ADT name for non-recursive types like Option
                let field_types = self.instantiate_constructor_fields_with_name(
                    &constructor.fields,
                    &type_params,
                    pattern_ty,
                    &adt_name,
                );

                self.wrap_nested_ctor_pattern(
                    body,
                    &constructor,
                    ctor_info.index,
                    &constructors,
                    sub_patterns,
                    &field_types,
                    value_var,
                    pattern_ty,
                    depth + 1,
                    pattern.span(),
                )
            }
            _ => Err(ElabError::unsupported(pattern.span(), "this pattern kind")),
        }
    }

    /// Wrap body with bindings for product (multi-field) sub-patterns.
    pub(super) fn wrap_product_subpatterns(
        &mut self,
        patterns: &[Pattern],
        field_types: &[Type],
        raw_var: &str,
        body: Term,
        depth: usize,
    ) -> ElabResult<Term> {
        let n = patterns.len();
        let mut result = body;

        // Process patterns in reverse order (innermost bindings first)
        for i in (0..n).rev() {
            // Build the accessor for field i
            let field_var = format!("__field{}_{}", depth, i);
            let mut accessor = Term::var(raw_var);
            for _ in 0..i {
                accessor = Term::snd(accessor);
            }
            if i < n - 1 {
                accessor = Term::fst(accessor);
            }

            // Wrap with pattern binding
            result = self.wrap_single_subpattern(
                &patterns[i],
                &field_types[i],
                &field_var,
                result,
                depth,
            )?;

            // Wrap with let binding for the field accessor
            result = Term::let_in(&field_var, field_types[i].clone(), accessor, result);
        }

        Ok(result)
    }

    /// Elaborate a product (multi-field) pattern that contains nested constructors.
    /// Collects bindings, elaborates body, then wraps.
    pub(super) fn elab_product_with_nested_ctors(
        &mut self,
        patterns: &[Pattern],
        field_types: &[Type],
        raw_var: &str,
        body_expr: &ast::Expr,
        depth: usize,
    ) -> ElabResult<Term> {
        // Phase 1: Collect all variable bindings from patterns
        let mut bindings: Vec<PatternBinding> = Vec::new();
        self.collect_pattern_bindings(patterns, field_types, &mut bindings, depth)?;

        // Bind all collected variables to the environment
        self.env.push_scope();
        for binding in &bindings {
            self.env
                .bind_local(binding.var_name.clone(), binding.var_ty.clone(), self.depth);
            self.depth += 1;
        }

        // Phase 2: Elaborate the body with bindings in scope
        let body_term = self.infer(body_expr)?.0;

        // Phase 3: Wrap the body with pattern destructs
        let wrapped =
            self.wrap_product_subpatterns(patterns, field_types, raw_var, body_term, depth)?;

        // Pop scope and depth
        for _ in &bindings {
            self.depth -= 1;
        }
        self.env.pop_scope();

        Ok(wrapped)
    }

    /// Build a case expression that extracts a specific constructor from a sum type.
    ///
    /// For the non-matching branches, generates absurd (bottom elimination).
    ///
    /// ## Representation Policy (ADR 2.2.26)
    ///
    /// - n = 1: Single constructor, just let binding
    /// - n = 2: Binary sum, nested case expressions
    /// - n >= 3: Flat ADT, use Term::adt_match direct extraction
    pub(super) fn build_ctor_extraction(
        &mut self,
        scrutinee: Term,
        sum_type: &Type,
        target_index: usize,
        num_ctors: usize,
        target_var: &str,
        target_body: Term,
        span: Span,
    ) -> ElabResult<Term> {
        // ADR 2.2.26: For flat ADT (n >= 3), use adt_match directly
        if let Type::Adt(_, _, variants) = sum_type {
            // Get the payload type for this variant
            let payload_ty = variants
                .get(target_index)
                .map(|(_, ty)| ty.clone())
                .unwrap_or(Type::Unit);

            // Build arms: target arm returns body, others return Sorry (unreachable)
            let arms: Vec<(usize, String, Box<Term>)> = (0..num_ctors)
                .map(|idx| {
                    if idx == target_index {
                        (idx, target_var.to_string(), Box::new(target_body.clone()))
                    } else {
                        // Other arms are unreachable in a let pattern
                        let dummy_var = format!("__unreachable_{}", idx);
                        (idx, dummy_var, Box::new(Term::Sorry))
                    }
                })
                .collect();

            return Ok(Term::adt_match(scrutinee, arms));
        }

        self.build_ctor_extraction_at(
            scrutinee,
            sum_type,
            target_index,
            num_ctors,
            0,
            target_var,
            target_body,
            span,
        )
    }

    /// Recursive helper for build_ctor_extraction.
    fn build_ctor_extraction_at(
        &mut self,
        scrutinee: Term,
        sum_type: &Type,
        target_index: usize,
        num_ctors: usize,
        current_index: usize,
        target_var: &str,
        target_body: Term,
        span: Span,
    ) -> ElabResult<Term> {
        if current_index == num_ctors - 1 {
            // Last position: this must be our target (no more rights to peel)
            if current_index != target_index {
                return Err(ElabError::new(
                    span,
                    ElabErrorKind::Other(
                        "internal error: reached end of sum without finding target constructor"
                            .to_string(),
                    ),
                ));
            }
            return Ok(Term::let_in(
                target_var,
                sum_type.clone(),
                scrutinee,
                target_body,
            ));
        }

        // Unwrap Mu if present
        let unwrapped = match sum_type {
            Type::Mu(_, body) => body.as_ref(),
            other => other,
        };

        // Get the left and right types of the current sum
        let (_left_ty, right_ty) = match unwrapped {
            Type::Sum(l, r) => (&**l, &**r),
            _ => {
                return Err(ElabError::new(
                    span,
                    ElabErrorKind::Other("expected sum type in constructor extraction".to_string()),
                ))
            }
        };

        if current_index == target_index {
            // This is our target constructor (the left branch)
            let right_var = format!("__abs{}", current_index);
            // For the right branch, we generate absurd (this case shouldn't happen
            // if pattern matching is correct, but we need a term)
            // We use a sorry/hole as a placeholder
            let absurd_body = Term::Sorry; // Placeholder, won't be evaluated

            Ok(Term::case(
                scrutinee,
                target_var,
                target_body,
                right_var,
                absurd_body,
            ))
        } else {
            // Target is in the right branch
            let left_var = format!("__abs{}", current_index);
            let absurd_body = Term::Sorry;

            let right_var = format!("__rest{}", current_index);

            // Recursively build the right branch
            let right_body = self.build_ctor_extraction_at(
                Term::var(&right_var),
                right_ty,
                target_index,
                num_ctors,
                current_index + 1,
                target_var,
                target_body,
                span,
            )?;

            Ok(Term::case(
                scrutinee,
                left_var,
                absurd_body,
                right_var,
                right_body,
            ))
        }
    }
}
