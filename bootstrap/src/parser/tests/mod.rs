//! Parser tests — split by topic for maintainability.

mod expressions;
mod items;
mod records;
mod use_statements;
mod visibility;

use super::*;

fn parse_ok(source: &str) -> SourceFile {
    let (file, errors) = parse(source);
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("{}", e);
        }
        panic!("Parse errors in: {}", source);
    }
    file
}

fn parse_has_errors(source: &str) -> bool {
    let wrapped = format!("fn test() {{ {} }}", source);
    let (_, errors) = parse(&wrapped);
    !errors.is_empty()
}

fn parse_expr_ok(source: &str) -> Expr {
    let wrapped = format!("fn test() {{ {} }}", source);
    let file = parse_ok(&wrapped);
    let body = &unwrap_fn(&file).body;
    match body {
        Expr::Block(_, Some(e), _) => *e.clone(),
        Expr::Block(stmts, None, _) if stmts.len() == 1 => {
            let Stmt::Expr(e, _) = &stmts[0] else {
                panic!("Expected expression statement");
            };
            e.clone()
        }
        other => other.clone(),
    }
}

fn unwrap_fn(file: &SourceFile) -> &FunctionDef {
    match &file.items[0] {
        Item::Function(f) => f,
        other => panic!("Expected function, got {:?}", other),
    }
}

fn unwrap_first_param_type(file: &SourceFile) -> &TypeExpr {
    &unwrap_fn(file).params[0].ty
}

fn unwrap_type_def(file: &SourceFile) -> &TypeDef {
    match &file.items[0] {
        Item::TypeDef(t) => t,
        other => panic!("Expected type def, got {:?}", other),
    }
}

fn unwrap_sum_variants(file: &SourceFile) -> &[Variant] {
    match &unwrap_type_def(file).body {
        TypeBody::Sum(variants) => variants,
        other => panic!("Expected sum type, got {:?}", other),
    }
}

fn unwrap_record_fields(file: &SourceFile) -> &[RecordField] {
    match &unwrap_type_def(file).body {
        TypeBody::Record(fields) => fields,
        other => panic!("Expected record type, got {:?}", other),
    }
}

fn unwrap_use_tree(file: &SourceFile) -> &UseTree {
    match &file.items[0] {
        Item::Use(u) => &u.tree,
        other => panic!("Expected use declaration, got {:?}", other),
    }
}

fn unwrap_use_decl(file: &SourceFile) -> &UseDecl {
    match &file.items[0] {
        Item::Use(u) => u,
        other => panic!("Expected use declaration, got {:?}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Type + Pattern + Error Recovery tests (small groups kept in mod.rs)
// ─────────────────────────────────────────────────────────────────────────

// Type tests

#[test]
fn test_arrow_type() {
    let file = parse_ok("fn test(f: Nat -> Bool) {}");
    assert!(
        matches!(unwrap_first_param_type(&file), TypeExpr::Arrow(_, _, _)),
        "Expected arrow type"
    );
}

#[test]
fn test_product_type() {
    let file = parse_ok("fn test(p: (Nat, Bool)) {}");
    assert!(
        matches!(unwrap_first_param_type(&file), TypeExpr::Product(_, _, _)),
        "Expected product type"
    );
}

#[test]
fn test_sum_type() {
    let file = parse_ok("type Opt = Some(Nat) | None");
    assert_eq!(unwrap_sum_variants(&file).len(), 2);
}

#[test]
fn test_forall_type() {
    let file = parse_ok("fn test(f: forall T. T -> T) {}");
    assert!(
        matches!(unwrap_first_param_type(&file), TypeExpr::Forall(_, _, _)),
        "Expected forall type"
    );
}

// Pattern tests

#[test]
fn test_wildcard_pattern() {
    let file = parse_ok("fn test() { match x { _ => 0 } }");
    assert_eq!(file.items.len(), 1);
}

#[test]
fn test_constructor_pattern() {
    let file = parse_ok("fn test() { match x { Some(y) => y, None => 0 } }");
    assert_eq!(file.items.len(), 1);
}

#[test]
fn test_or_pattern() {
    let file = parse_ok("fn test() { match x { 0 | 1 => true, _ => false } }");
    assert_eq!(file.items.len(), 1);
}

#[test]
fn test_tuple_pattern() {
    let file = parse_ok("fn test() { match p { (x, y) => x + y } }");
    assert_eq!(file.items.len(), 1);
}

// Eq type-position tests

#[test]
fn test_eq_explicit_return_type() {
    let file = parse_ok("fn f() -> Eq<Nat, 0, 0> { refl }");
    let ret = unwrap_fn(&file).return_type.as_ref().expect("return type");
    assert!(
        matches!(ret, TypeExpr::EqExplicit(_, _, _, _)),
        "Expected EqExplicit, got {:?}",
        ret
    );
}

#[test]
fn test_eq_explicit_param_type() {
    let file = parse_ok("fn f(p: Eq<Nat, 0, 0>) {}");
    assert!(
        matches!(
            unwrap_first_param_type(&file),
            TypeExpr::EqExplicit(_, _, _, _)
        ),
        "Expected EqExplicit"
    );
}

// == sugar tests

#[test]
fn test_eq_sugar_int_return_type() {
    let file = parse_ok("fn f() -> 0 == 0 { refl }");
    let ret = unwrap_fn(&file).return_type.as_ref().expect("return type");
    assert!(
        matches!(ret, TypeExpr::Eq(_, _, _)),
        "Expected Eq (sugar), got {:?}",
        ret
    );
}

#[test]
fn test_eq_sugar_bool_return_type() {
    let file = parse_ok("fn f() -> true == true { refl }");
    let ret = unwrap_fn(&file).return_type.as_ref().expect("return type");
    assert!(
        matches!(ret, TypeExpr::Eq(_, _, _)),
        "Expected Eq (sugar), got {:?}",
        ret
    );
}

#[test]
fn test_eq_sugar_ident_return_type() {
    let file = parse_ok("fn f(x: Nat) -> x == x { refl }");
    let ret = unwrap_fn(&file).return_type.as_ref().expect("return type");
    assert!(
        matches!(ret, TypeExpr::Eq(_, _, _)),
        "Expected Eq (sugar), got {:?}",
        ret
    );
}

#[test]
fn test_eq_sugar_fn_call_return_type() {
    let file = parse_ok("fn f(n: Nat) -> succ(n) == n + 1 { refl }");
    let ret = unwrap_fn(&file).return_type.as_ref().expect("return type");
    assert!(
        matches!(ret, TypeExpr::Eq(_, _, _)),
        "Expected Eq (sugar), got {:?}",
        ret
    );
}

// Error recovery tests

#[test]
fn test_error_recovery() {
    let (file, errors) = parse("fn foo() {} @ fn bar() {}");
    assert!(errors.len() >= 1);
    assert!(!file.items.is_empty());
}

#[test]
fn test_multiple_items() {
    let file = parse_ok(
        r#"
        fn foo() { 1 }
        fn bar() { 2 }
        type MyType = Unit
        theorem test: Bool { true }
        "#,
    );
    assert_eq!(file.items.len(), 4);
}

#[test]
fn test_reserved_keyword_in_expr_does_not_hang() {
    let (file, errors) = parse("fn main() -> Nat { const(1, 2) }");
    assert!(!errors.is_empty());
    assert!(!file.items.is_empty());
}

#[test]
fn test_reserved_keyword_as_function_name() {
    let (_file, errors) = parse("fn const() -> Nat { 42 }");
    assert!(!errors.is_empty());
    assert!(errors
        .iter()
        .any(|e| matches!(e.kind, ParseErrorKind::ReservedKeyword(_))));
}
