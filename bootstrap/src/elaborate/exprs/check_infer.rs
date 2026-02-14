//! Bidirectional type checking entry points.
//!
//! Implements:
//! - `check(expr, expected)` - check expression against known type
//! - `infer(expr)` - synthesize type from expression

use crate::ast::Expr;
use crate::span::Spanned;
use tungsten_core::{Term, Type};

use crate::elaborate::env::{ModulePath, PathResolutionError, ResolvedValue};
use crate::elaborate::error::{ElabError, ElabErrorKind, ExpectedContext};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Check an expression against an expected type.
    ///
    /// Use this when we know what type the expression should have.
    pub fn check(&mut self, expr: &Expr, expected: &Type) -> ElabResult<Term> {
        match expr {
            // Lambda: if checking against A → B, bind param as A, check body against B
            Expr::Lambda(params, body, span) => self.check_lambda(params, body, expected, *span),

            // If: check condition as Bool, check both branches against expected
            Expr::If(cond, then_branch, else_branch, _span) => {
                let cond_term = self.check(cond, &Type::Bool)?;
                let then_term = self.check(then_branch, expected)?;
                let else_term = self.check(else_branch, expected)?;
                Ok(Term::if_then_else(cond_term, then_term, else_term))
            }

            // Block: elaborate statements, check final expression against expected
            Expr::Block(stmts, final_expr, span) => {
                self.check_block(stmts, final_expr.as_deref(), Some(expected), *span)
            }

            // Let: infer value type, bind, check body against expected
            Expr::Let(pattern, ty_ann, value, body, span) => {
                let (term, _ty) =
                    self.elab_let(pattern, ty_ann.as_ref(), value, body, Some(expected), *span)?;
                Ok(term)
            }

            // Have (proof sugar): have h: P = proof; body
            Expr::Have(name, prop, proof, body, span) => {
                let (term, _ty) = self.elab_have(name, prop, proof, body, Some(expected), *span)?;
                Ok(term)
            }

            // Show (type ascription): show P { proof }
            Expr::Show(prop, proof, span) => {
                let (term, _ty) = self.elab_show(prop, proof, Some(expected), *span)?;
                Ok(term)
            }

            // Assume (lambda intro): assume h: P; body
            Expr::Assume(name, prop, body, span) => {
                let (term, _ty) = self.elab_assume(name, prop, body, Some(expected), *span)?;
                Ok(term)
            }

            // Match: infer scrutinee, check arms against expected
            Expr::Match(scrutinee, arms, span) => {
                let (term, _ty) = self.elab_match(scrutinee, arms, Some(expected), *span)?;
                Ok(term)
            }

            // Record literal: use expected type to determine field types
            Expr::RecordLit {
                spread,
                fields,
                span,
            } => self.elab_record_literal(spread.as_deref(), fields, expected, *span),

            // Sorry: accepts any expected type (axiom-like hole)
            Expr::Sorry(_) => Ok(Term::Sorry),

            // Constructor: use expected type to determine type arguments
            Expr::Path(path) => {
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

                // Check if this is a constructor using path resolution
                let resolution =
                    self.env
                        .resolve_value_path(path, self.depth, &self.current_module);
                match resolution {
                    Ok(Some(ResolvedValue::Constructor(info))) => {
                        let name = path.item_name().name.clone();
                        // Check constructor visibility (inherits from parent type in v1)
                        if !self
                            .env
                            .is_constructor_accessible(&info, &self.current_module, true)
                        {
                            if let Some(item_module) = self.env.get_item_module(&info.type_name) {
                                return Err(ElabError::private_item(
                                    path.span,
                                    &name,
                                    "constructor",
                                    item_module.to_string(),
                                    self.current_module.to_string(),
                                ));
                            }
                        }
                        return self.check_constructor_ref(&name, &info, expected, path.span);
                    }
                    Ok(Some(_)) | Ok(None) => {
                        // Not a constructor or not found - fall through to infer
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
                }
                // Not a constructor, fall through to default
                let (term, inferred) = self.infer(expr)?;
                if !self.types_equal(&inferred, expected) {
                    return Err(self.type_mismatch_error(expr.span(), expected.clone(), inferred));
                }
                Ok(term)
            }

            // Constructor application: use expected type to determine type arguments
            Expr::App(func, args, span) => {
                // Check if this is a constructor application
                if let Expr::Path(path) = func.as_ref() {
                    if path.is_simple() {
                        let ident = path.item_name();
                        if let Some(info) = self.env.lookup_constructor(&ident.name).cloned() {
                            // Check constructor visibility (inherits from parent type in v1)
                            if !self.env.is_constructor_accessible(
                                &info,
                                &self.current_module,
                                true,
                            ) {
                                if let Some(item_module) = self.env.get_item_module(&info.type_name)
                                {
                                    return Err(ElabError::private_item(
                                        path.span,
                                        &ident.name,
                                        "constructor",
                                        item_module.to_string(),
                                        self.current_module.to_string(),
                                    ));
                                }
                            }
                            return self.check_constructor_application(
                                &ident.name,
                                &info,
                                args,
                                expected,
                                *span,
                            );
                        }
                    }
                }
                // Not a constructor, fall through to default
                let (term, inferred) = self.infer(expr)?;
                if !self.types_equal(&inferred, expected) {
                    return Err(self.type_mismatch_error(expr.span(), expected.clone(), inferred));
                }
                Ok(term)
            }

            // Tuple: propagate expected type into elements
            Expr::Tuple(elems, span) => self.check_tuple(elems, expected, *span),

            // Default: infer type, check it matches expected
            _ => {
                let (term, inferred) = self.infer(expr)?;
                if !self.types_equal(&inferred, expected) {
                    return Err(self.type_mismatch_error(expr.span(), expected.clone(), inferred));
                }
                Ok(term)
            }
        }
    }

    /// Infer the type of an expression.
    ///
    /// Use this when we don't know what type to expect.
    /// Returns both the elaborated term and its type.
    pub fn infer(&mut self, expr: &Expr) -> ElabResult<(Term, Type)> {
        match expr {
            // ─────────────────────────────────────────────────────────────────
            // Literals
            // ─────────────────────────────────────────────────────────────────
            Expr::IntLiteral(n, _span) => {
                // Build Nat: 0 → Zero, n → Succ^n(Zero)
                let term = self.nat_literal(*n);
                Ok((term, Type::Nat))
            }

            Expr::BoolLiteral(b, _span) => {
                let term = if *b { Term::True } else { Term::False };
                Ok((term, Type::Bool))
            }

            Expr::Unit(_span) => Ok((Term::Unit, Type::Unit)),

            Expr::StringLiteral(s, _span) => {
                // Phase 2A: String literals are now supported
                Ok((Term::string_lit(s.clone()), Type::String))
            }

            // ─────────────────────────────────────────────────────────────────
            // Variables
            // ─────────────────────────────────────────────────────────────────
            Expr::Path(path) => {
                let name = &path.item_name().name;

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

                // Use path resolution (handles both simple and qualified paths)
                match self
                    .env
                    .resolve_value_path(path, self.depth, &self.current_module)
                {
                    Ok(Some(ResolvedValue::Local(_idx, ty))) => {
                        // Local variable: use name (will be substituted later)
                        // Locals don't need visibility checks
                        Ok((Term::var(name), ty))
                    }
                    Ok(Some(ResolvedValue::Global(global_name, ty))) => {
                        // Check item visibility
                        if let Some(value_def) = self.env.lookup_value(&global_name) {
                            if let Some(item_module) = self.env.get_item_module(&global_name) {
                                if !self.env.is_item_accessible(
                                    value_def.visibility,
                                    item_module,
                                    &self.current_module,
                                    true,
                                ) {
                                    return Err(ElabError::private_item(
                                        path.span,
                                        &global_name,
                                        "function",
                                        item_module.to_string(),
                                        self.current_module.to_string(),
                                    ));
                                }
                            }
                        }
                        // Global: emit Term::Global for environment-based evaluation
                        Ok((Term::global(global_name), ty))
                    }
                    Ok(Some(ResolvedValue::Constructor(info))) => {
                        // Check constructor visibility (inherits from parent type in v1)
                        if !self
                            .env
                            .is_constructor_accessible(&info, &self.current_module, true)
                        {
                            if let Some(item_module) = self.env.get_item_module(&info.type_name) {
                                return Err(ElabError::private_item(
                                    path.span,
                                    name,
                                    "constructor",
                                    item_module.to_string(),
                                    self.current_module.to_string(),
                                ));
                            }
                        }
                        // Constructor: need to build injection
                        self.elab_constructor_ref(name, &info, path.span)
                    }
                    Ok(None) => {
                        if path.is_simple() {
                            Err(self.undefined_variable_error(path.span, name))
                        } else {
                            let module_str = path
                                .module_segments()
                                .iter()
                                .map(|s| s.name.as_str())
                                .collect::<Vec<_>>()
                                .join("::");
                            Err(ElabError::item_not_in_module(path.span, module_str, name))
                        }
                    }
                    Err(PathResolutionError::ModuleNotFound(module)) => {
                        Err(ElabError::module_not_found(path.span, module.to_string()))
                    }
                    Err(PathResolutionError::ItemNotFound { module, item }) => Err(
                        ElabError::item_not_in_module(path.span, module.to_string(), item),
                    ),
                }
            }

            // ─────────────────────────────────────────────────────────────────
            // Lambda
            // ─────────────────────────────────────────────────────────────────
            Expr::Lambda(params, body, span) => {
                // For inference, all parameters must have type annotations
                self.infer_lambda(params, body, *span)
            }

            // ─────────────────────────────────────────────────────────────────
            // Application
            // ─────────────────────────────────────────────────────────────────
            Expr::App(func, args, span) => self.elab_application(func, args, *span),

            // ─────────────────────────────────────────────────────────────────
            // Binary operators
            // ─────────────────────────────────────────────────────────────────
            Expr::Binary(left, op, right, span) => self.elab_binary(left, *op, right, *span),

            // ─────────────────────────────────────────────────────────────────
            // Unary operators
            // ─────────────────────────────────────────────────────────────────
            Expr::Unary(op, operand, span) => self.elab_unary(*op, operand, *span),

            // ─────────────────────────────────────────────────────────────────
            // Let binding
            // ─────────────────────────────────────────────────────────────────
            Expr::Let(pattern, ty_ann, value, body, span) => {
                self.elab_let(pattern, ty_ann.as_ref(), value, body, None, *span)
            }

            // ─────────────────────────────────────────────────────────────────
            // If expression
            // ─────────────────────────────────────────────────────────────────
            Expr::If(cond, then_branch, else_branch, _span) => {
                let cond_term = self.check(cond, &Type::Bool)?;
                let (then_term, then_ty) = self.infer(then_branch)?;
                // Push context so errors in else branch reference the then branch
                self.push_context(ExpectedContext::branch_unification(then_branch.span()));
                let else_term = self.check(else_branch, &then_ty)?;
                self.pop_context();
                Ok((Term::if_then_else(cond_term, then_term, else_term), then_ty))
            }

            // ─────────────────────────────────────────────────────────────────
            // Match expression
            // ─────────────────────────────────────────────────────────────────
            Expr::Match(scrutinee, arms, span) => self.elab_match(scrutinee, arms, None, *span),

            // ─────────────────────────────────────────────────────────────────
            // Block
            // ─────────────────────────────────────────────────────────────────
            Expr::Block(stmts, final_expr, span) => {
                self.infer_block(stmts, final_expr.as_deref(), *span)
            }

            // ─────────────────────────────────────────────────────────────────
            // Tuple
            // ─────────────────────────────────────────────────────────────────
            Expr::Tuple(elems, span) => self.elab_tuple(elems, *span),

            // ─────────────────────────────────────────────────────────────────
            // Type annotation
            // ─────────────────────────────────────────────────────────────────
            Expr::Annot(inner, ty, _span) => {
                let expected = self.elab_type(ty)?;
                let term = self.check(inner, &expected)?;
                Ok((term, expected))
            }

            // ─────────────────────────────────────────────────────────────────
            // Type application
            // ─────────────────────────────────────────────────────────────────
            Expr::TypeApp(func, type_args, span) => self.elab_expr_type_app(func, type_args, *span),

            // ─────────────────────────────────────────────────────────────────
            // Proof constructs
            // ─────────────────────────────────────────────────────────────────
            Expr::Have(name, prop, proof, body, span) => {
                self.elab_have(name, prop, proof, body, None, *span)
            }

            Expr::Show(prop, proof, span) => self.elab_show(prop, proof, None, *span),

            Expr::Assume(name, prop, body, span) => self.elab_assume(name, prop, body, None, *span),

            Expr::Refl(span) => {
                // refl without annotation: cannot infer
                Err(ElabError::cannot_infer(*span)
                    .with_help("add type annotation: `refl : Eq<T, x, x>`"))
            }

            Expr::Sorry(sorry) => {
                // sorry without expected type: use Unit as placeholder
                // This will likely cause a type error later
                Err(ElabError::cannot_infer(sorry.span)
                    .with_help("add type annotation: `sorry : T`"))
            }

            // ─────────────────────────────────────────────────────────────────
            // Unsupported in Phase 1
            // ─────────────────────────────────────────────────────────────────
            Expr::RecordLit { span, .. } => {
                // Record literals require an expected type
                Err(ElabError::cannot_infer(*span)
                    .with_help("add type annotation: `{ x: 1, y: 2 } : Point`"))
            }

            Expr::Field(base, field, span) => {
                // Field access is now supported
                self.elab_field_access(base, field, *span)
            }

            Expr::Return(_, span) => Err(ElabError::return_not_supported(*span)),

            Expr::Paren(inner, _span) => self.infer(inner),

            Expr::Error(span) => Err(ElabError::new(
                *span,
                ElabErrorKind::Other("syntax error".to_string()),
            )),
        }
    }
}
