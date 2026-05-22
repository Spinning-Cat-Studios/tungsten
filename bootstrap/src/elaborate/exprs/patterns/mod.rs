//! Nested constructor pattern elaboration (Phase C).
//!
//! Handles:
//! - `elab_nested_ctor_pattern` - elaborate nested constructor patterns
//! - `collect_pattern_bindings` - gather variable bindings from patterns
//! - `elab_product_with_nested_ctors` - elaborate product patterns with nested ctors

mod wrapping;

use crate::ast::{self, Pattern};
use crate::config::MAX_PATTERN_DEPTH;
use crate::span::Spanned;
use tungsten_core::{Term, Type};

use super::helpers::PatternBinding;
use crate::elaborate::env::{self as elab_env, ModulePath, PathResolutionError};
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

/// Resolved constructor information for pattern matching.
/// Bundles the results of constructor path resolution, type lookup, and ADT validation.
struct ResolvedCtor {
    /// The constructor's index in the ADT
    index: usize,
    /// The ADT's constructors
    constructors: Vec<elab_env::Constructor>,
    /// The ADT's type parameters
    type_params: Vec<String>,
    /// The parent type name
    type_name: String,
}

impl<'a> Elaborator<'a> {
    /// Resolve a constructor path for pattern matching:
    /// checks module visibility, resolves the path, looks up the type, and validates it's an ADT.
    fn resolve_pattern_ctor(
        &self,
        ctor_path: &ast::Path,
        pattern_span: crate::span::Span,
    ) -> ElabResult<ResolvedCtor> {
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

        // Resolve constructor path
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

        // Look up type definition and verify it's an ADT
        let Some(type_def) = self.env.lookup_type(&ctor_info.type_name).cloned() else {
            return Err(ElabError::new(
                pattern_span,
                ElabErrorKind::Other(format!("type '{}' not found", ctor_info.type_name)),
            ));
        };
        let elab_env::TypeDefKind::ADT(constructors) = type_def.kind else {
            return Err(ElabError::new(
                pattern_span,
                ElabErrorKind::Other(format!("'{}' is not an ADT", ctor_info.type_name)),
            ));
        };

        Ok(ResolvedCtor {
            index: ctor_info.index,
            constructors,
            type_params: type_def.params,
            type_name: ctor_info.type_name,
        })
    }

    /// Elaborate a nested constructor pattern by:
    /// 1. First, collect all variable bindings and bind them in the environment
    /// 2. Then, elaborate the body with those bindings in scope
    /// 3. Finally, wrap the body with the appropriate destructs and cases
    ///
    /// Returns the wrapped body term.
    pub(in crate::elaborate) fn elab_nested_ctor_pattern(
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

        match pattern {
            Pattern::Constructor(ref ctor_path, ref sub_patterns, _) => {
                let resolved = self.resolve_pattern_ctor(ctor_path, pattern.span())?;
                let constructor = &resolved.constructors[resolved.index];

                // Instantiate constructor field types with proper two-phase substitution
                // (type params first, then μ-type unfolding - see ADR 24.1.26)
                // Use explicit ADT name for non-recursive types like Option
                let field_types = self.instantiate_constructor_fields_with_name(
                    &constructor.fields,
                    &resolved.type_params,
                    value_ty,
                    &resolved.type_name,
                );

                // Phase 1: Collect all variable bindings from the patterns
                let mut bindings: Vec<PatternBinding> = Vec::new();
                self.collect_pattern_bindings(sub_patterns, &field_types, &mut bindings, depth)?;

                // Bind all collected variables into the environment
                self.env.push_scope();
                for binding in &bindings {
                    self.env.bind_local(
                        binding.var_name.clone(),
                        binding.var_ty.clone(),
                        self.depth,
                    );
                    self.depth += 1;
                }

                // Phase 2: Elaborate the body with bindings in scope
                let body_term = self.infer(body_expr)?.0;

                // Phase 3: Wrap the body with pattern destructs
                let ctor_ctx = wrapping::CtorPatternCtx {
                    type_name: &resolved.type_name,
                    constructor: &resolved.constructors[resolved.index],
                    ctor_index: resolved.index,
                    constructors: &resolved.constructors,
                    sub_patterns,
                    field_types: &field_types,
                    value_var,
                    value_ty,
                };

                let wrapped =
                    self.wrap_nested_ctor_pattern(body_term, &ctor_ctx, depth, pattern.span())?;

                // Pop scope + depth
                for _ in &bindings {
                    self.depth -= 1;
                }
                self.env.pop_scope();

                Ok(wrapped)
            }
            _ => Err(ElabError::new(
                pattern.span(),
                ElabErrorKind::Other(format!("expected constructor pattern, got {:?}", pattern)),
            )),
        }
    }

    /// Collect variable bindings from a list of patterns.
    pub(in crate::elaborate) fn collect_pattern_bindings(
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
                let resolved = self.resolve_pattern_ctor(ctor_path, pattern.span())?;
                let constructor = &resolved.constructors[resolved.index];

                // Instantiate constructor field types with proper two-phase substitution
                // (type params first, then μ-type unfolding - see ADR 24.1.26)
                // Use explicit ADT name for non-recursive types like Option
                let field_types = self.instantiate_constructor_fields_with_name(
                    &constructor.fields,
                    &resolved.type_params,
                    pattern_ty,
                    &resolved.type_name,
                );

                // Recursively collect from sub-patterns
                self.collect_pattern_bindings(sub_patterns, &field_types, bindings, depth + 1)?;
            }
            Pattern::Tuple(ref sub_pats, tup_span) => {
                // Tuple inside constructor (ADR 15.5.26f) — extract element
                // types and recursively collect bindings depth-first.
                // Tuples are irrefutable, so they do NOT increment the depth
                // counter (which bounds constructor-nesting complexity).
                let elem_types = self.extract_tuple_types(pattern_ty, sub_pats.len(), *tup_span)?;
                for (sub_pat, elem_ty) in sub_pats.iter().zip(elem_types.iter()) {
                    self.collect_binding_from_pattern(sub_pat, elem_ty, bindings, depth)?;
                }
            }
            _ => {
                return Err(ElabError::unsupported(pattern.span(), "this pattern kind"));
            }
        }
        Ok(())
    }

    /// Elaborate a product (multi-field) pattern that contains nested constructors.
    /// Collects bindings, elaborates body, then wraps.
    pub(in crate::elaborate) fn elab_product_with_nested_ctors(
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
}
