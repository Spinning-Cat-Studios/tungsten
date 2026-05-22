//! Export Validation (Phase 3: Public Item Leak Detection)
//!
//! Validates that public item signatures don't leak private types.
//! A "visibility leak" occurs when a public item's signature references
//! a type that is less visible than the item itself.

use std::collections::HashSet;

use crate::ast::{self, Item, TypeBody, TypeExpr, Visibility};

use crate::elaborate::env::Env;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::Elaborator;

/// Identity of a public item being checked for visibility leaks.
struct ItemIdentity<'a> {
    span: crate::span::Span,
    name: &'a str,
    kind: &'a str,
    visibility: Visibility,
}

/// Information about a visibility leak.
pub struct VisibilityLeak {
    /// Path showing how the private type was reached
    pub path: Vec<String>,
    /// The actual visibility of the leaked type
    pub visibility: Visibility,
}

impl<'a> Elaborator<'a> {
    /// Validate that all public item signatures don't leak private types.
    ///
    /// This should be called after the collection pass (Pass 1) completes,
    /// before the elaboration pass (Pass 2).
    ///
    /// A "visibility leak" occurs when a public item's signature references
    /// a type that is less visible than the item itself. For example:
    /// - `pub fn foo() -> PrivateType` leaks `PrivateType`
    /// - `pub type Alias = PrivateType` leaks `PrivateType`
    ///
    /// NOTE: We check AST types, not resolved Core types, because aliases
    /// are resolved during elaboration. For example, `type A = B` stores
    /// the resolved type of B, not a reference to B.
    pub fn validate_export_signatures(&self, items: &[Item]) -> Vec<ElabError> {
        let mut errors = Vec::new();

        for item in items {
            match item {
                Item::Function(func) => {
                    if func.visibility != Visibility::Private {
                        self.check_function_visibility(func, &mut errors);
                    }
                }
                Item::TypeDef(type_def) => {
                    if type_def.visibility != Visibility::Private {
                        self.check_type_def_visibility(type_def, &mut errors);
                    }
                }
                Item::TypeAlias(alias) => {
                    if alias.visibility != Visibility::Private {
                        self.check_type_alias_visibility(alias, &mut errors);
                    }
                }
                Item::Theorem(thm) | Item::Lemma(thm) => {
                    if thm.visibility != Visibility::Private {
                        let item = ItemIdentity {
                            span: thm.span,
                            name: &thm.name.name,
                            kind: "theorem",
                            visibility: thm.visibility,
                        };
                        self.check_proposition_item_visibility(
                            &item,
                            &thm.params,
                            &thm.prop,
                            &mut errors,
                        );
                    }
                }
                Item::Axiom(axiom) => {
                    if axiom.visibility != Visibility::Private {
                        let item = ItemIdentity {
                            span: axiom.span,
                            name: &axiom.name.name,
                            kind: "axiom",
                            visibility: axiom.visibility,
                        };
                        self.check_proposition_item_visibility(
                            &item,
                            &axiom.params,
                            &axiom.prop,
                            &mut errors,
                        );
                    }
                }
                Item::ExternFn(extern_fn) => {
                    if extern_fn.visibility != Visibility::Private {
                        self.check_extern_fn_visibility(extern_fn, &mut errors);
                    }
                }
                _ => {}
            }
        }

        errors
    }

    /// Check a function's signature for visibility leaks.
    fn check_function_visibility(&self, func: &ast::FunctionDef, errors: &mut Vec<ElabError>) {
        let required = func.visibility;
        let mut visited = HashSet::new();
        let base_path = vec![func.name.name.clone()];

        // Check parameter types
        for (i, param) in func.params.iter().enumerate() {
            let mut path = base_path.clone();
            path.push(format!("param {}", i + 1));
            if let Some(leak) =
                self.check_ast_type_visibility(&param.ty, required, &mut visited, &mut path)
            {
                errors.push(self.make_leak_error(
                    func.span,
                    &func.name.name,
                    "function",
                    required,
                    leak,
                ));
            }
        }

        // Check return type
        if let Some(ref ret_ty) = func.return_type {
            let mut path = base_path.clone();
            path.push("return type".to_string());
            if let Some(leak) =
                self.check_ast_type_visibility(ret_ty, required, &mut visited, &mut path)
            {
                errors.push(self.make_leak_error(
                    func.span,
                    &func.name.name,
                    "function",
                    required,
                    leak,
                ));
            }
        }
    }

    /// Check a type definition's constructors for visibility leaks.
    fn check_type_def_visibility(&self, type_def: &ast::TypeDef, errors: &mut Vec<ElabError>) {
        let required = type_def.visibility;
        let mut visited = HashSet::new();
        let base_path = vec![type_def.name.name.clone()];

        // Collect field types from both Sum and Record bodies
        let fields: Vec<(&TypeExpr, String)> = match &type_def.body {
            TypeBody::Sum(variants) => variants
                .iter()
                .flat_map(|v| {
                    let name = &v.name.name;
                    v.fields
                        .iter()
                        .enumerate()
                        .map(move |(i, f)| (&f.ty, format!("{}::field {}", name, i)))
                })
                .collect(),
            TypeBody::Record(fields) => fields
                .iter()
                .map(|f| (&f.ty, format!("field `{}`", f.name.name)))
                .collect(),
        };

        for (ty, label) in &fields {
            let mut path = base_path.clone();
            path.push(label.clone());
            if let Some(leak) =
                self.check_ast_type_visibility(ty, required, &mut visited, &mut path)
            {
                errors.push(self.make_leak_error(
                    type_def.span,
                    &type_def.name.name,
                    "type",
                    required,
                    leak,
                ));
                return; // One error per item is enough
            }
        }
    }

    /// Check a type alias for visibility leaks.
    fn check_type_alias_visibility(&self, alias: &ast::TypeAlias, errors: &mut Vec<ElabError>) {
        let required = alias.visibility;
        let mut visited = HashSet::new();
        let mut path = vec![alias.name.name.clone()];

        if let Some(leak) =
            self.check_ast_type_visibility(&alias.ty, required, &mut visited, &mut path)
        {
            errors.push(self.make_leak_error(
                alias.span,
                &alias.name.name,
                "type alias",
                required,
                leak,
            ));
        }
    }

    /// Check a proposition item's (theorem/lemma/axiom) signature for visibility leaks.
    fn check_proposition_item_visibility(
        &self,
        item: &ItemIdentity<'_>,
        params: &[ast::Param],
        prop: &TypeExpr,
        errors: &mut Vec<ElabError>,
    ) {
        let required = item.visibility;
        let mut visited = HashSet::new();
        let base_path = vec![item.name.to_string()];

        // Check parameter types
        for (i, param) in params.iter().enumerate() {
            let mut path = base_path.clone();
            path.push(format!("param {}", i + 1));
            if let Some(leak) =
                self.check_ast_type_visibility(&param.ty, required, &mut visited, &mut path)
            {
                errors.push(self.make_leak_error(item.span, item.name, item.kind, required, leak));
                return;
            }
        }

        // Check proposition type
        let mut path = base_path;
        path.push("proposition".to_string());
        if let Some(leak) = self.check_ast_type_visibility(prop, required, &mut visited, &mut path)
        {
            errors.push(self.make_leak_error(item.span, item.name, item.kind, required, leak));
        }
    }

    /// Check an extern function's signature for visibility leaks.
    fn check_extern_fn_visibility(
        &self,
        extern_fn: &ast::ExternFnDef,
        errors: &mut Vec<ElabError>,
    ) {
        let required = extern_fn.visibility;
        let mut visited = HashSet::new();
        let base_path = vec![extern_fn.name.name.clone()];

        // Check parameter types
        for param in &extern_fn.params {
            let mut path = base_path.clone();
            path.push(format!("param `{}`", param.name.name));
            if let Some(leak) =
                self.check_ast_type_visibility(&param.ty, required, &mut visited, &mut path)
            {
                errors.push(self.make_leak_error(
                    extern_fn.span,
                    &extern_fn.name.name,
                    "extern function",
                    required,
                    leak,
                ));
                return;
            }
        }

        // Check return type (always present for extern fn)
        let mut path = base_path;
        path.push("return type".to_string());
        if let Some(leak) = self.check_ast_type_visibility(
            &extern_fn.return_type,
            required,
            &mut visited,
            &mut path,
        ) {
            errors.push(self.make_leak_error(
                extern_fn.span,
                &extern_fn.name.name,
                "extern function",
                required,
                leak,
            ));
        }
    }

    /// Check an AST type expression for visibility leaks.
    ///
    /// Returns `Some(VisibilityLeak)` if a leak is found, `None` otherwise.
    fn check_ast_type_visibility(
        &self,
        ty: &TypeExpr,
        required: Visibility,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<VisibilityLeak> {
        match ty {
            TypeExpr::Path(type_path) => {
                let name = type_path.item_name().name.clone();

                // Skip built-in types
                if matches!(
                    name.as_str(),
                    "Nat" | "Bool" | "Unit" | "Void" | "Prop" | "String"
                ) {
                    return None;
                }

                // Look up the type
                if let Some(type_def) = self.env.lookup_type(&name) {
                    // Check visibility
                    if !Env::visibility_at_least(type_def.visibility, required) {
                        path.push(name.clone());
                        return Some(VisibilityLeak {
                            path: path.clone(),
                            visibility: type_def.visibility,
                        });
                    }
                }

                None
            }

            // Compound types - check recursively
            TypeExpr::Arrow(t1, t2, _)
            | TypeExpr::Product(t1, t2, _)
            | TypeExpr::Sum(t1, t2, _) => self
                .check_ast_type_visibility(t1, required, visited, path)
                .or_else(|| self.check_ast_type_visibility(t2, required, visited, path)),
            TypeExpr::App(base, args, _) => {
                // Check the base type
                if let Some(leak) = self.check_ast_type_visibility(base, required, visited, path) {
                    return Some(leak);
                }
                // Check type arguments
                for arg in args {
                    if let Some(leak) = self.check_ast_type_visibility(arg, required, visited, path)
                    {
                        return Some(leak);
                    }
                }
                None
            }
            TypeExpr::Forall(_, body, _) => {
                self.check_ast_type_visibility(body, required, visited, path)
            }
            TypeExpr::Ptr(inner, _) | TypeExpr::Ref(inner, _) | TypeExpr::Paren(inner, _) => {
                self.check_ast_type_visibility(inner, required, visited, path)
            }

            // Built-in type expressions
            TypeExpr::Prop(_) | TypeExpr::Unit(_) | TypeExpr::Void(_) => None,

            // Eq types - we only check the type parts (terms don't leak types)
            TypeExpr::Eq(_, _, _) => None, // Terms don't have type visibility concerns
            TypeExpr::EqExplicit(ty, _, _, _) => {
                self.check_ast_type_visibility(ty, required, visited, path)
            }

            // Error nodes
            TypeExpr::Error(_) => None,
        }
    }

    /// Create a visibility leak error.
    fn make_leak_error(
        &self,
        span: crate::span::Span,
        item_name: &str,
        item_kind: &str,
        required: Visibility,
        leak: VisibilityLeak,
    ) -> ElabError {
        ElabError::new(
            span,
            ElabErrorKind::PublicItemLeak {
                item_name: item_name.to_string(),
                item_kind: item_kind.to_string(),
                required_visibility: Env::visibility_name(required).to_string(),
                leak_path: leak.path,
                leaked_visibility: Env::visibility_name(leak.visibility).to_string(),
            },
        )
    }
}
