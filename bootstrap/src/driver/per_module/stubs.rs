//! Phase A stub collection helpers (ADR 5.5.26c §2.2).
//!
//! These functions convert parsed AST items into shallow stub definitions
//! for Phase A of per-module elaboration. Stubs provide enough type structure
//! for cross-branch import resolution and pattern matching.

use crate::ast::{Item, TypeBody};
use crate::elaborate::{Constructor, ConstructorInfo, ModuleExports, TypeDef, TypeDefKind};
use tungsten_core::Type;

use crate::driver::modules::ParsedModule;
/// Convert an AST TypeExpr to a placeholder Core Type for Phase A stubs.
///
/// Uses built-in types where possible and `TyVar` for named types. This is a
/// shallow translation — no elaboration — that gives the elaborator enough
/// type structure for generic instantiation and pattern matching.
pub(super) fn type_expr_to_placeholder(ty: &crate::ast::TypeExpr) -> Type {
    use crate::ast::TypeExpr;
    match ty {
        TypeExpr::Path(path) if path.segments.len() == 1 => type_expr_to_alias_target(ty)
            .unwrap_or_else(|| Type::TyVar(path.segments[0].name.clone())),
        TypeExpr::Path(path) => {
            let name = path.segments.last().map_or("_", |s| &s.name);
            Type::TyVar(name.to_string())
        }
        TypeExpr::Arrow(a, b, _) => Type::Arrow(
            Box::new(type_expr_to_placeholder(a)),
            Box::new(type_expr_to_placeholder(b)),
        ),
        TypeExpr::Product(a, b, _) => Type::Product(
            Box::new(type_expr_to_placeholder(a)),
            Box::new(type_expr_to_placeholder(b)),
        ),
        TypeExpr::Unit(_) => Type::Unit,
        TypeExpr::Void(_) => Type::Void,
        _ => Type::Unit, // fallback for complex expressions
    }
}

/// Try to resolve a simple type alias target from an AST TypeExpr.
///
/// Handles built-in types (`Nat`, `Bool`, `String`, `Unit`, `Void`) and
/// pointer types. Returns `None` for complex type expressions that require
/// full elaboration.
pub(super) fn type_expr_to_alias_target(ty: &crate::ast::TypeExpr) -> Option<Type> {
    use crate::ast::TypeExpr;
    match ty {
        TypeExpr::Path(path) if path.segments.len() == 1 => {
            builtin_type_by_name(&path.segments[0].name)
        }
        TypeExpr::Unit(_) => Some(Type::Unit),
        TypeExpr::Void(_) => Some(Type::Void),
        TypeExpr::Ptr(inner, _) => Some(Type::Ptr(Box::new(
            type_expr_to_alias_target(inner).unwrap_or(Type::Unit),
        ))),
        _ => None,
    }
}

/// Map a type name to the corresponding built-in Core type.
fn builtin_type_by_name(name: &str) -> Option<Type> {
    match name {
        "Nat" => Some(Type::Nat),
        "Bool" => Some(Type::Bool),
        "String" => Some(Type::String),
        "Unit" => Some(Type::Unit),
        "Void" => Some(Type::Void),
        "Prop" => Some(Type::Prop),
        _ => None,
    }
}

/// Collect a single type definition stub for Phase A.
fn collect_type_def_stub(t: &crate::ast::TypeDef, exports: &mut ModuleExports) {
    let name = t.name.name.clone();
    let params: Vec<String> = t.type_params.iter().map(|p| p.name.name.clone()).collect();

    let kind = match &t.body {
        TypeBody::Sum(variants) => {
            let ctors: Vec<Constructor> = variants
                .iter()
                .enumerate()
                .map(|(index, v)| Constructor {
                    name: v.name.name.clone(),
                    fields: v
                        .fields
                        .iter()
                        .map(|f| type_expr_to_placeholder(&f.ty))
                        .collect(),
                    index,
                    visibility: v.visibility,
                    span: v.span,
                })
                .collect();
            TypeDefKind::ADT(ctors)
        }
        TypeBody::Record(fields) => {
            let rec_fields: Vec<(String, Type)> = fields
                .iter()
                .map(|f| (f.name.name.clone(), type_expr_to_placeholder(&f.ty)))
                .collect();
            TypeDefKind::Record(rec_fields)
        }
    };

    exports.types.push((
        name.clone(),
        TypeDef {
            name: name.clone(),
            params,
            kind,
            visibility: t.visibility,
            span: t.span,
            defining_module: None,
            encoded_type: None,
            field_visibilities: match &t.body {
                TypeBody::Record(fields) => fields.iter().map(|f| f.visibility).collect(),
                _ => Vec::new(),
            },
        },
    ));

    if let TypeBody::Sum(variants) = &t.body {
        for (index, variant) in variants.iter().enumerate() {
            exports.constructors.push((
                variant.name.name.clone(),
                ConstructorInfo {
                    type_name: name.clone(),
                    index,
                    arity: variant.fields.len(),
                    visibility: variant.visibility,
                    defining_module: None,
                },
            ));
        }
    }
}

/// Collect a single type alias stub for Phase A.
fn collect_type_alias_stub(t: &crate::ast::TypeAlias, exports: &mut ModuleExports) {
    let name = t.name.name.clone();
    let params: Vec<String> = t.type_params.iter().map(|p| p.name.name.clone()).collect();

    let kind = if params.is_empty() {
        match type_expr_to_alias_target(&t.ty) {
            Some(ty) => TypeDefKind::Alias(ty),
            None => TypeDefKind::Stub,
        }
    } else {
        TypeDefKind::Stub
    };

    exports.types.push((
        name,
        TypeDef {
            name: t.name.name.clone(),
            params,
            kind,
            visibility: t.visibility,
            span: t.span,
            defining_module: None,
            encoded_type: None,
            field_visibilities: Vec::new(),
        },
    ));
}

/// Phase A: collect type and constructor stubs from all modules (ADR 5.5.26c §2.2).
///
/// Walks the entire module tree and registers shallow type stubs and ADT
/// constructor stubs into `exports`. This gives every module cross-branch
/// type visibility before any value-body elaboration begins.
pub(crate) fn collect_all_type_and_constructor_stubs(
    tree: &ParsedModule,
    exports: &mut ModuleExports,
) {
    for item in &tree.source_file.items {
        match item {
            Item::TypeDef(t) => collect_type_def_stub(t, exports),
            Item::TypeAlias(t) => collect_type_alias_stub(t, exports),
            _ => {}
        }
    }

    for child in &tree.submodules {
        collect_all_type_and_constructor_stubs(child, exports);
    }
}
