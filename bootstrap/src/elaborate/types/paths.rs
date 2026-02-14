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
            match name.as_str() {
                "Nat" => return Ok(Type::Nat),
                "Bool" => return Ok(Type::Bool),
                "Unit" => return Ok(Type::Unit),
                "Void" => return Ok(Type::Void),
                "Prop" => return Ok(Type::Prop),
                "String" => return Ok(Type::String),
                _ => {}
            }

            // Check if it's a type variable in scope (only for simple paths)
            if self.env.has_type_var(name) {
                return Ok(Type::TyVar(name.to_string()));
            }
        }

        // Try path resolution (handles both simple and qualified paths)
        // Clone the data we need to avoid borrowing issues
        let resolution_result = self.env.resolve_type_path(path, &self.current_module);
        let (type_name, params_len, kind, visibility) = match resolution_result {
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
                (
                    type_def.name.clone(),
                    type_def.params.len(),
                    type_def.kind.clone(),
                    type_def.visibility,
                )
            }
            Ok(None) => {
                // Not found
                if path.is_simple() {
                    return Err(self.undefined_type_error(span, name));
                } else {
                    let module_str = path
                        .module_segments()
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join("::");
                    return Err(ElabError::item_not_in_module(span, module_str, name));
                }
            }
            Err(PathResolutionError::ModuleNotFound(module)) => {
                return Err(ElabError::module_not_found(span, module.to_string()));
            }
            Err(PathResolutionError::ItemNotFound { module, item }) => {
                return Err(ElabError::item_not_in_module(
                    span,
                    module.to_string(),
                    item,
                ));
            }
        };

        // Check item visibility (for both simple and qualified paths)
        if let Some(item_module) = self.env.get_item_module(&type_name) {
            if !self
                .env
                .is_item_accessible(visibility, item_module, &self.current_module, true)
            {
                return Err(ElabError::private_item(
                    span,
                    &type_name,
                    "type",
                    item_module.to_string(),
                    self.current_module.to_string(),
                ));
            }
        }

        // If it has no parameters, we can use it directly
        if params_len == 0 {
            match &kind {
                TypeDefKind::Alias(ty) => Ok(ty.clone()),
                TypeDefKind::ADT(_) => {
                    // For ADTs, we encode them as sum types
                    self.encode_adt_type(&type_name, &[])
                }
                TypeDefKind::Record(_) => {
                    // For records, return the nominal type (TyVar)
                    // The encoding to Product happens at codegen time
                    // This preserves record identity for field access resolution
                    Ok(Type::TyVar(type_name))
                }
                TypeDefKind::Stub => {
                    // Stub type - this happens during cross-module type elaboration
                    // when types are defined after the type that references them.
                    //
                    // IMPORTANT (ADR 30.1.26.1): Stubs may have incorrect params (Vec::new())
                    // if created during module discovery before actual type was elaborated.
                    // We must re-resolve through imports to find the actual param count.
                    if let Some(actual_params) = self.resolve_stub_actual_params(&type_name) {
                        if !actual_params.is_empty() {
                            // Actual type has params but none provided - arity error
                            return Err(ElabError::new(
                                span,
                                ElabErrorKind::ArityMismatch {
                                    expected: actual_params.len(),
                                    found: 0,
                                },
                            )
                            .with_note(format!(
                                "`{}` requires {} type parameter(s)",
                                name,
                                actual_params.len()
                            )));
                        }
                    }
                    // Either no actual def found, or it truly has 0 params
                    Ok(Type::TyVar(type_name))
                }
            }
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
                        // For ADTs, we encode them as sum types
                        self.encode_adt_type(name, &[])
                    }
                    TypeDefKind::Record(_) => {
                        // For records, return the nominal type (TyVar)
                        // The encoding to Product happens at codegen time
                        Ok(Type::TyVar(name.to_string()))
                    }
                    TypeDefKind::Stub => {
                        // Stub type - return a type variable that will be resolved later
                        Ok(Type::TyVar(name.to_string()))
                    }
                };
            } else {
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

        // Look up the type definition using path resolution
        let (params, kind, expected_arity, type_name, visibility) = {
            let type_def = match self.env.resolve_type_path(base_path, &self.current_module) {
                Ok(Some(td)) => {
                    // For qualified paths, check module visibility
                    if !base_path.is_simple() {
                        let module_path = ModulePath::new(
                            base_path
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
                                base_path.span,
                                module_path.to_string(),
                                self.current_module.to_string(),
                            ));
                        }
                    }
                    td
                }
                Ok(None) => {
                    if base_path.is_simple() {
                        return Err(self.undefined_type_error(base_path.span, name));
                    } else {
                        let module_str = base_path
                            .module_segments()
                            .iter()
                            .map(|s| s.name.as_str())
                            .collect::<Vec<_>>()
                            .join("::");
                        return Err(ElabError::item_not_in_module(
                            base_path.span,
                            module_str,
                            name,
                        ));
                    }
                }
                Err(PathResolutionError::ModuleNotFound(module)) => {
                    return Err(ElabError::module_not_found(
                        base_path.span,
                        module.to_string(),
                    ));
                }
                Err(PathResolutionError::ItemNotFound { module, item }) => {
                    return Err(ElabError::item_not_in_module(
                        base_path.span,
                        module.to_string(),
                        item,
                    ));
                }
            };
            (
                type_def.params.clone(),
                type_def.kind.clone(),
                type_def.params.len(),
                type_def.name.clone(),
                type_def.visibility,
            )
        };

        // IMPORTANT (ADR 30.1.26.1): For stubs, the params may be incorrect (Vec::new())
        // if created during module discovery. Re-resolve to find actual arity.
        let expected_arity = if matches!(&kind, TypeDefKind::Stub) {
            self.resolve_stub_actual_params(&type_name)
                .map(|params| params.len())
                .unwrap_or(expected_arity)
        } else {
            expected_arity
        };

        // Check item visibility
        if let Some(item_module) = self.env.get_item_module(&type_name) {
            if !self
                .env
                .is_item_accessible(visibility, item_module, &self.current_module, true)
            {
                return Err(ElabError::private_item(
                    span,
                    &type_name,
                    "type",
                    item_module.to_string(),
                    self.current_module.to_string(),
                ));
            }
        }

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
                // Encode ADT with type arguments
                self.encode_adt_type(name, &arg_types)
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
