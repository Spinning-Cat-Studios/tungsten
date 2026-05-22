//! Type Path and Name Resolution
//!
//! This module handles resolving type paths (both simple and qualified)
//! to their corresponding type definitions.

use crate::ast::TypeExpr;
use crate::elaborate::env::{ModulePath, PathResolutionError, TypeDefKind};
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::ElabResult;
use crate::elaborate::Elaborator;
use crate::span::Spanned;
use tungsten_core::Type;

impl<'a> Elaborator<'a> {
    /// Elaborate a type path (simple or qualified).
    pub(super) fn elab_type_path(&mut self, path: &crate::ast::Path) -> ElabResult<Type> {
        let name = &path.item_name().name;
        let span = path.span;

        // Check for built-in types first (only for simple paths)
        if path.is_simple() {
            if let Some(ty) = Self::builtin_type(name) {
                return Ok(ty);
            }

            // Check if it's a type variable in scope (only for simple paths)
            if self.env.has_type_var(name) {
                return Ok(Type::TyVar(name.to_string()));
            }
        }

        // Resolve path to type definition
        let (type_name, params_len, kind, visibility) = self.resolve_type_path_checked(path)?;

        // Check item visibility
        self.check_type_item_visibility(&type_name, visibility, span)?;

        // If it has no parameters, we can use it directly
        if params_len == 0 {
            self.type_def_to_type(name, &type_name, &kind, span)
        } else {
            // Type requires parameters
            Err(ElabError::new(
                span,
                ElabErrorKind::ArityMismatch {
                    expected: params_len,
                    found: 0,
                },
            )
            .with_note(format!(
                "`{}` requires {} type parameter(s)",
                name, params_len
            )))
        }
    }

    /// Map a builtin type name to its Type, if any.
    fn builtin_type(name: &str) -> Option<Type> {
        match name {
            "Nat" => Some(Type::Nat),
            "Bool" => Some(Type::Bool),
            "Unit" => Some(Type::Unit),
            "Void" => Some(Type::Void),
            "Prop" => Some(Type::Prop),
            "String" => Some(Type::String),
            _ => None,
        }
    }

    /// Resolve a type path to its definition info, checking module visibility.
    fn resolve_type_path_checked(
        &self,
        path: &crate::ast::Path,
    ) -> ElabResult<(String, usize, TypeDefKind, crate::ast::Visibility)> {
        let span = path.span;
        let name = &path.item_name().name;

        let resolution_result = self.env.resolve_type_path(path, &self.current_module);
        match resolution_result {
            Ok(Some(type_def)) => {
                // For qualified paths, check module visibility
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
                            span,
                            module_path.to_string(),
                            self.current_module.to_string(),
                        ));
                    }
                }
                Ok((
                    type_def.name.clone(),
                    type_def.params.len(),
                    type_def.kind.clone(),
                    type_def.visibility,
                ))
            }
            Ok(None) => {
                if path.is_simple() {
                    Err(self.undefined_type_error(span, name))
                } else {
                    let module_str = path
                        .module_segments()
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join("::");
                    Err(ElabError::item_not_in_module(span, module_str, name))
                }
            }
            Err(PathResolutionError::ModuleNotFound(module)) => {
                Err(ElabError::module_not_found(span, module.to_string()))
            }
            Err(PathResolutionError::ItemNotFound { module, item }) => Err(
                ElabError::item_not_in_module(span, module.to_string(), item),
            ),
        }
    }

    /// Check type item visibility, returning an error if private.
    fn check_type_item_visibility(
        &self,
        type_name: &str,
        visibility: crate::ast::Visibility,
        span: crate::span::Span,
    ) -> ElabResult<()> {
        if let Some(item_module) = self.env.get_item_module(type_name) {
            // Re-export visibility capping (ADR 14.5.26c §2.3)
            let vis =
                self.env
                    .effective_type_visibility(type_name, visibility, &self.current_module);
            if !self
                .env
                .is_item_accessible(vis, item_module, &self.current_module, true)
            {
                return Err(ElabError::private_item(
                    span,
                    type_name,
                    "type",
                    item_module.to_string(),
                    self.current_module.to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Convert a resolved zero-param type def to a Type.
    fn type_def_to_type(
        &mut self,
        display_name: &str,
        type_name: &str,
        kind: &TypeDefKind,
        span: crate::span::Span,
    ) -> ElabResult<Type> {
        match kind {
            TypeDefKind::Alias(ty) => Ok(ty.clone()),
            TypeDefKind::ADT(_) => {
                // During Phase 1c (collection), defer ADT encoding as TyVar("@Name")
                // so mutual recursion groups can be computed first (ADR 18.4.26i §5).
                // Phase 1d will resolve these to proper encodings.
                if self.collection_phase {
                    Ok(Type::TyVar(format!("@{}", type_name)))
                } else {
                    self.encode_adt_type(type_name, &[])
                }
            }
            TypeDefKind::Record(_) => Ok(Type::TyVar(format!("@{}", type_name))),
            TypeDefKind::Stub => {
                // IMPORTANT (ADR 30.1.26.1): Stubs may have incorrect params
                if let Some(actual_params) = self.resolve_stub_actual_params(type_name) {
                    if !actual_params.is_empty() {
                        return Err(ElabError::new(
                            span,
                            ElabErrorKind::ArityMismatch {
                                expected: actual_params.len(),
                                found: 0,
                            },
                        )
                        .with_note(format!(
                            "`{}` requires {} type parameter(s)",
                            display_name,
                            actual_params.len()
                        )));
                    }
                }
                Ok(Type::TyVar(format!("@{}", type_name)))
            }
        }
    }

    /// Elaborate a type name (identifier).
    /// Note: This is kept for reference but elab_type_path is now the primary method.
    #[allow(dead_code)]
    pub(super) fn elab_type_name(
        &mut self,
        name: &str,
        span: crate::span::Span,
    ) -> ElabResult<Type> {
        // Check for built-in types first
        match name {
            "Nat" => return Ok(Type::Nat),
            "Bool" => return Ok(Type::Bool),
            "Unit" => return Ok(Type::Unit),
            "Void" => return Ok(Type::Void),
            "Prop" => return Ok(Type::Prop),
            "String" => return Ok(Type::String), // Phase 2A
            _ => {}
        }

        // Check if it's a defined type BEFORE checking type variables.
        // This ensures that ADT names like `LexErrors` resolve to their
        // μ-type encoding, not to a type variable that might be in scope
        // during recursive type definition.
        if let Some(type_def) = self.env.lookup_type(name) {
            // If it has no parameters, we can use it directly
            if type_def.params.is_empty() {
                return match &type_def.kind {
                    TypeDefKind::Alias(ty) => Ok(ty.clone()),
                    TypeDefKind::ADT(_) => {
                        // During Phase 1c, defer ADT encoding (ADR 18.4.26i §5)
                        if self.collection_phase {
                            Ok(Type::TyVar(format!("@{}", name)))
                        } else {
                            self.encode_adt_type(name, &[])
                        }
                    }
                    TypeDefKind::Record(_) => {
                        // For records, return the nominal type (TyVar) with @-prefix
                        // to distinguish from genuine type variables (ADR 13.4.26c §2)
                        Ok(Type::TyVar(format!("@{}", name)))
                    }
                    TypeDefKind::Stub => {
                        // Stub type with @-prefix (ADR 13.4.26c §2)
                        Ok(Type::TyVar(format!("@{}", name)))
                    }
                };
            }
            // Type requires parameters
            return Err(ElabError::new(
                span,
                ElabErrorKind::ArityMismatch {
                    expected: type_def.params.len(),
                    found: 0,
                },
            )
            .with_note(format!(
                "`{}` requires {} type parameter(s)",
                name,
                type_def.params.len()
            )));
        }

        // Check if it's a type variable in scope
        // This comes AFTER defined types so that recursive ADT references
        // during type definition (which use the ADT name as a type var)
        // still work correctly.
        if self.env.has_type_var(name) {
            return Ok(Type::TyVar(name.to_string()));
        }

        // Not found
        Err(self.undefined_type_error(span, name))
    }

    /// Elaborate a type application (e.g., `List<Nat>`).
    pub(super) fn elab_type_app(
        &mut self,
        base: &TypeExpr,
        args: &[TypeExpr],
        span: crate::span::Span,
    ) -> ElabResult<Type> {
        // Get the base type name
        let TypeExpr::Path(base_path) = base else {
            return Err(ElabError::new(
                base.span(),
                ElabErrorKind::Other("expected type name in type application".to_string()),
            ));
        };

        let name = &base_path.item_name().name;

        // Check if it's a type variable (for recursive type definitions)
        // In that case, `List<T>` where `List` is a type var becomes just `List`
        // (the type arguments are already handled via the type params)
        if base_path.is_simple() && self.env.has_type_var(name) {
            // For recursive self-references like `List<T>` inside `type List<T> = ...`,
            // we just return the type variable. The type args will be substituted later
            // when the type is used.
            return Ok(Type::TyVar(name.to_string()));
        }

        // Look up the type definition using shared path resolution
        let (type_name, params_len, kind, visibility) =
            self.resolve_type_path_checked(base_path)?;

        // Get the params for alias substitution
        let params =
            if let Ok(Some(td)) = self.env.resolve_type_path(base_path, &self.current_module) {
                td.params.clone()
            } else {
                Vec::new()
            };

        // IMPORTANT (ADR 30.1.26.1): For stubs, the params may be incorrect (Vec::new())
        // if created during module discovery. Re-resolve to find actual arity.
        let expected_arity = if matches!(&kind, TypeDefKind::Stub) {
            self.resolve_stub_actual_params(&type_name)
                .map(|params| params.len())
                .unwrap_or(params_len)
        } else {
            params_len
        };

        // Check item visibility
        self.check_type_item_visibility(&type_name, visibility, span)?;

        // Check arity
        if expected_arity != args.len() {
            return Err(ElabError::arity_mismatch(span, expected_arity, args.len()));
        }

        // Elaborate argument types
        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.elab_type(arg)?);
        }

        // Apply the type arguments
        match &kind {
            TypeDefKind::Alias(ty) => {
                // Substitute type parameters
                let mut result = ty.clone();
                for (param, arg) in params.iter().zip(arg_types.iter()) {
                    result = result.substitute(param, arg);
                }
                Ok(result)
            }
            TypeDefKind::ADT(_) => {
                // During Phase 1c, defer ADT encoding (ADR 18.4.26i §5)
                if self.collection_phase {
                    Ok(Type::app(name.to_string(), arg_types))
                } else {
                    self.encode_adt_type(name, &arg_types)
                }
            }
            TypeDefKind::Record(_) => {
                // For records, return the nominal type as Type::App
                // (Records don't support type parameters currently,
                // but use App for consistency with ADTs)
                // The encoding to Product happens at codegen time
                Ok(Type::app(type_name, arg_types))
            }
            TypeDefKind::Stub => {
                // Stub type with type arguments - create a deferred application
                // This will be resolved in Phase 1d after all types are elaborated
                Ok(Type::app(name.to_string(), arg_types))
            }
        }
    }

    /// Resolve a stub type to find the actual type definition's parameters.
    ///
    /// Stubs created during module discovery may have incorrect params (Vec::new())
    /// because the actual type definition wasn't available yet. This method follows
    /// imports to find the real type definition and returns its actual parameters.
    ///
    /// See ADR 30.1.26.1 for details on this fix.
    fn resolve_stub_actual_params(&self, type_name: &str) -> Option<Vec<String>> {
        // 1. Check if there's an import for this type in the current module
        if let Some(import_info) = self.env.lookup_type_import(&self.current_module, type_name) {
            // 2. Resolve to the source module's actual type definition
            if let Some(actual_def) = self
                .env
                .lookup_type_in_module(&import_info.source_module, &import_info.original_name)
            {
                // Only use if it's not also a stub (avoid infinite loops)
                if !matches!(actual_def.kind, TypeDefKind::Stub) {
                    return Some(actual_def.params.clone());
                }
            }
        }

        // 3. Also try direct lookup - the type might have been elaborated after the stub was created
        if let Some(type_def) = self.env.lookup_type(type_name) {
            if !matches!(type_def.kind, TypeDefKind::Stub) {
                return Some(type_def.params.clone());
            }
        }

        None
    }
}
