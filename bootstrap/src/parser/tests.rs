//! Parser tests.

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

fn parse_expr_ok(source: &str) -> Expr {
    let wrapped = format!("fn test() {{ {} }}", source);
    let file = parse_ok(&wrapped);
    match &file.items[0] {
        Item::Function(f) => match &f.body {
            Expr::Block(_, Some(e), _) => *e.clone(),
            Expr::Block(stmts, None, _) if stmts.len() == 1 => match &stmts[0] {
                Stmt::Expr(e, _) => e.clone(),
                _ => panic!("Expected expression statement"),
            },
            other => other.clone(),
        },
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_empty_file() {
    let file = parse_ok("");
    assert!(file.items.is_empty());
}

#[test]
fn test_simple_function() {
    let file = parse_ok("fn foo() { 42 }");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "foo");
            assert!(f.params.is_empty());
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_function_with_params() {
    let file = parse_ok("fn add(x: Nat, y: Nat) -> Nat { x + y }");
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "add");
            assert_eq!(f.params.len(), 2);
            assert!(f.return_type.is_some());
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_function_with_type_params() {
    let file = parse_ok("fn id<T>(x: T) -> T { x }");
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "id");
            assert_eq!(f.type_params.len(), 1);
            assert_eq!(f.type_params[0].name.name, "T");
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_type_alias() {
    let file = parse_ok("type MyInt = Nat");
    match &file.items[0] {
        Item::TypeAlias(t) => {
            assert_eq!(t.name.name, "MyInt");
        }
        _ => panic!("Expected type alias"),
    }
}

#[test]
fn test_type_def() {
    let file = parse_ok("type Option<T> = None | Some(T)");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Option");
            assert_eq!(t.type_params.len(), 1);
            match &t.body {
                TypeBody::Sum(variants) => assert_eq!(variants.len(), 2),
                _ => panic!("Expected sum type"),
            }
        }
        _ => panic!("Expected type def"),
    }
}

#[test]
fn test_struct() {
    let file = parse_ok("struct Point { x: Nat, y: Nat }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Point");
            match &t.body {
                TypeBody::Sum(variants) => {
                    assert_eq!(variants.len(), 1);
                    assert_eq!(variants[0].fields.len(), 2);
                }
                _ => panic!("Expected sum type (struct becomes single variant)"),
            }
        }
        _ => panic!("Expected type def"),
    }
}

#[test]
fn test_enum() {
    let file = parse_ok("enum Color { Red, Green, Blue }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Color");
            match &t.body {
                TypeBody::Sum(variants) => assert_eq!(variants.len(), 3),
                _ => panic!("Expected sum type"),
            }
        }
        _ => panic!("Expected type def"),
    }
}

#[test]
fn test_theorem() {
    let file = parse_ok("theorem trivial: Bool { true }");
    match &file.items[0] {
        Item::Theorem(t) => {
            assert_eq!(t.name.name, "trivial");
        }
        _ => panic!("Expected theorem"),
    }
}

#[test]
fn test_theorem_with_params() {
    // Use Eq<Nat, x, x> syntax per ADR for equality types
    let file = parse_ok("theorem reflexive(x: Nat) -> Eq<Nat, x, x> { sorry }");
    match &file.items[0] {
        Item::Theorem(t) => {
            assert_eq!(t.name.name, "reflexive");
            assert_eq!(t.params.len(), 1);
        }
        _ => panic!("Expected theorem"),
    }
}

#[test]
fn test_lemma() {
    let file = parse_ok("lemma helper: Prop { sorry }");
    match &file.items[0] {
        Item::Lemma(t) => {
            assert_eq!(t.name.name, "helper");
        }
        _ => panic!("Expected lemma"),
    }
}

#[test]
fn test_axiom() {
    let file = parse_ok("axiom excluded_middle(P: Prop): P");
    match &file.items[0] {
        Item::Axiom(a) => {
            assert_eq!(a.name.name, "excluded_middle");
        }
        _ => panic!("Expected axiom"),
    }
}

#[test]
fn test_mod_declaration() {
    let file = parse_ok("mod foo;");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "foo");
            assert_eq!(m.visibility, Visibility::Private);
        }
        _ => panic!("Expected mod declaration"),
    }
}

#[test]
fn test_pub_mod_declaration() {
    let file = parse_ok("pub mod api;");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "api");
            assert_eq!(m.visibility, Visibility::Public);
        }
        _ => panic!("Expected pub mod declaration"),
    }
}

#[test]
fn test_mod_visibility_mixed() {
    let file = parse_ok("pub mod api;\nmod internal;\npub mod types;");
    assert_eq!(file.items.len(), 3);

    match &file.items[0] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "api");
            assert_eq!(m.visibility, Visibility::Public);
        }
        _ => panic!("Expected mod declaration"),
    }

    match &file.items[1] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "internal");
            assert_eq!(m.visibility, Visibility::Private);
        }
        _ => panic!("Expected mod declaration"),
    }

    match &file.items[2] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "types");
            assert_eq!(m.visibility, Visibility::Public);
        }
        _ => panic!("Expected mod declaration"),
    }
}

#[test]
fn test_mod_multiple() {
    let file = parse_ok("mod foo;\nmod bar;\nfn main() { 1 }");
    assert_eq!(file.items.len(), 3);
    assert!(
        matches!(&file.items[0], Item::Mod(m) if m.name.name == "foo" && m.visibility == Visibility::Private)
    );
    assert!(
        matches!(&file.items[1], Item::Mod(m) if m.name.name == "bar" && m.visibility == Visibility::Private)
    );
    assert!(matches!(&file.items[2], Item::Function(_)));
}

// Use statement tests

#[test]
fn test_use_simple() {
    let file = parse_ok("use foo::bar;");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Use(u) => {
            assert_eq!(u.visibility, Visibility::Private);
            match &u.tree {
                UseTree::Path(path) => {
                    assert_eq!(path.segments.len(), 2);
                    assert_eq!(path.segments[0].name, "foo");
                    assert_eq!(path.segments[1].name, "bar");
                }
                _ => panic!("Expected path use tree"),
            }
        }
        _ => panic!("Expected use declaration"),
    }
}

#[test]
fn test_use_three_segments() {
    let file = parse_ok("use api::types::Config;");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Use(u) => match &u.tree {
            UseTree::Path(path) => {
                assert_eq!(path.segments.len(), 3);
                assert_eq!(path.segments[0].name, "api");
                assert_eq!(path.segments[1].name, "types");
                assert_eq!(path.segments[2].name, "Config");
            }
            _ => panic!("Expected path use tree"),
        },
        _ => panic!("Expected use declaration"),
    }
}

#[test]
fn test_use_grouped() {
    let file = parse_ok("use api::{Config, Error};");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Use(u) => {
            assert_eq!(u.visibility, Visibility::Private);
            match &u.tree {
                UseTree::Group { prefix, items, .. } => {
                    assert_eq!(prefix.segments.len(), 1);
                    assert_eq!(prefix.segments[0].name, "api");
                    assert_eq!(items.len(), 2);
                    match &items[0] {
                        UseTree::Path(p) => assert_eq!(p.segments[0].name, "Config"),
                        _ => panic!("Expected path"),
                    }
                    match &items[1] {
                        UseTree::Path(p) => assert_eq!(p.segments[0].name, "Error"),
                        _ => panic!("Expected path"),
                    }
                }
                _ => panic!("Expected group use tree"),
            }
        }
        _ => panic!("Expected use declaration"),
    }
}

#[test]
fn test_use_grouped_trailing_comma() {
    let file = parse_ok("use api::{Config, Error,};");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Use(u) => match &u.tree {
            UseTree::Group { items, .. } => {
                assert_eq!(items.len(), 2);
            }
            _ => panic!("Expected group use tree"),
        },
        _ => panic!("Expected use declaration"),
    }
}

#[test]
fn test_pub_use() {
    let file = parse_ok("pub use internal::Helper;");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Use(u) => {
            assert_eq!(u.visibility, Visibility::Public);
            match &u.tree {
                UseTree::Path(path) => {
                    assert_eq!(path.segments.len(), 2);
                    assert_eq!(path.segments[0].name, "internal");
                    assert_eq!(path.segments[1].name, "Helper");
                }
                _ => panic!("Expected path use tree"),
            }
        }
        _ => panic!("Expected use declaration"),
    }
}

#[test]
fn test_use_tree_expand() {
    use crate::ast::ExpandedUseTree;
    let file = parse_ok("use api::types::{Config, Error};");
    match &file.items[0] {
        Item::Use(u) => match u.tree.expand() {
            ExpandedUseTree::Paths(expanded) => {
                assert_eq!(expanded.len(), 2);
                assert_eq!(expanded[0].segments.len(), 3);
                assert_eq!(expanded[0].segments[0].name, "api");
                assert_eq!(expanded[0].segments[1].name, "types");
                assert_eq!(expanded[0].segments[2].name, "Config");
                assert_eq!(expanded[1].segments.len(), 3);
                assert_eq!(expanded[1].segments[0].name, "api");
                assert_eq!(expanded[1].segments[1].name, "types");
                assert_eq!(expanded[1].segments[2].name, "Error");
            }
            ExpandedUseTree::Glob { .. } => panic!("Expected paths, got glob"),
        },
        _ => panic!("Expected use declaration"),
    }
}

#[test]
fn test_use_glob() {
    let file = parse_ok("use foo::*;");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Use(u) => match &u.tree {
            UseTree::Glob { prefix, .. } => {
                assert_eq!(prefix.segments.len(), 1);
                assert_eq!(prefix.segments[0].name, "foo");
            }
            _ => panic!("Expected glob use tree"),
        },
        _ => panic!("Expected use declaration"),
    }
}

#[test]
fn test_use_glob_deep() {
    let file = parse_ok("use foo::bar::baz::*;");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Use(u) => match &u.tree {
            UseTree::Glob { prefix, .. } => {
                assert_eq!(prefix.segments.len(), 3);
                assert_eq!(prefix.segments[0].name, "foo");
                assert_eq!(prefix.segments[1].name, "bar");
                assert_eq!(prefix.segments[2].name, "baz");
            }
            _ => panic!("Expected glob use tree"),
        },
        _ => panic!("Expected use declaration"),
    }
}

#[test]
fn test_use_glob_expand() {
    use crate::ast::ExpandedUseTree;
    let file = parse_ok("use api::types::*;");
    match &file.items[0] {
        Item::Use(u) => match u.tree.expand() {
            ExpandedUseTree::Glob { prefix, .. } => {
                assert_eq!(prefix.segments.len(), 2);
                assert_eq!(prefix.segments[0].name, "api");
                assert_eq!(prefix.segments[1].name, "types");
            }
            ExpandedUseTree::Paths(_) => panic!("Expected glob, got paths"),
        },
        _ => panic!("Expected use declaration"),
    }
}

#[test]
fn test_pub_use_glob() {
    let file = parse_ok("pub use internal::*;");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Use(u) => {
            assert_eq!(u.visibility, Visibility::Public);
            match &u.tree {
                UseTree::Glob { prefix, .. } => {
                    assert_eq!(prefix.segments.len(), 1);
                    assert_eq!(prefix.segments[0].name, "internal");
                }
                _ => panic!("Expected glob use tree"),
            }
        }
        _ => panic!("Expected use declaration"),
    }
}

// Expression tests

#[test]
fn test_int_literal() {
    let e = parse_expr_ok("42");
    match e {
        Expr::IntLiteral(v, _) => assert_eq!(v, 42),
        _ => panic!("Expected int literal"),
    }
}

#[test]
fn test_hex_literal() {
    let e = parse_expr_ok("0x2A");
    match e {
        Expr::IntLiteral(v, _) => assert_eq!(v, 42),
        _ => panic!("Expected int literal"),
    }
}

#[test]
fn test_bool_literal() {
    let e = parse_expr_ok("true");
    match e {
        Expr::BoolLiteral(v, _) => assert!(v),
        _ => panic!("Expected bool literal"),
    }
}

#[test]
fn test_string_literal() {
    let e = parse_expr_ok(r#""hello""#);
    match e {
        Expr::StringLiteral(v, _) => assert_eq!(v, "hello"),
        _ => panic!("Expected string literal"),
    }
}

#[test]
fn test_variable() {
    let e = parse_expr_ok("x");
    match e {
        Expr::Path(path) => {
            assert!(path.is_simple());
            assert_eq!(path.item_name().name, "x");
        }
        _ => panic!("Expected path"),
    }
}

#[test]
fn test_qualified_path() {
    let e = parse_expr_ok("foo::bar::baz");
    match e {
        Expr::Path(path) => {
            assert!(!path.is_simple());
            assert_eq!(path.segments.len(), 3);
            assert_eq!(path.segments[0].name, "foo");
            assert_eq!(path.segments[1].name, "bar");
            assert_eq!(path.segments[2].name, "baz");
            assert_eq!(path.item_name().name, "baz");
        }
        _ => panic!("Expected path"),
    }
}

#[test]
fn test_qualified_path_two_segments() {
    let e = parse_expr_ok("module::item");
    match e {
        Expr::Path(path) => {
            assert!(!path.is_simple());
            assert_eq!(path.segments.len(), 2);
            assert_eq!(path.segments[0].name, "module");
            assert_eq!(path.segments[1].name, "item");
        }
        _ => panic!("Expected path"),
    }
}

#[test]
fn test_binary_ops() {
    let e = parse_expr_ok("1 + 2 * 3");
    match e {
        Expr::Binary(left, BinOp::Add, right, _) => {
            assert!(matches!(*left, Expr::IntLiteral(1, _)));
            assert!(matches!(*right, Expr::Binary(_, BinOp::Mul, _, _)));
        }
        _ => panic!("Expected binary expression"),
    }
}

#[test]
fn test_comparison() {
    let e = parse_expr_ok("x == y");
    match e {
        Expr::Binary(_, BinOp::Eq, _, _) => {}
        _ => panic!("Expected equality"),
    }
}

#[test]
fn test_logical_ops() {
    let e = parse_expr_ok("a && b || c");
    match e {
        Expr::Binary(_, BinOp::Or, _, _) => {}
        _ => panic!("Expected or expression"),
    }
}

#[test]
fn test_unary_not() {
    let e = parse_expr_ok("!x");
    match e {
        Expr::Unary(UnaryOp::Not, _, _) => {}
        _ => panic!("Expected not expression"),
    }
}

#[test]
fn test_unary_neg() {
    let e = parse_expr_ok("-42");
    match e {
        Expr::Unary(UnaryOp::Neg, _, _) => {}
        _ => panic!("Expected negation"),
    }
}

#[test]
fn test_function_call() {
    let e = parse_expr_ok("foo(1, 2)");
    match e {
        Expr::App(func, args, _) => {
            assert!(matches!(*func, Expr::Path(_)));
            assert_eq!(args.len(), 2);
        }
        _ => panic!("Expected function call"),
    }
}

#[test]
fn test_if_expr() {
    let e = parse_expr_ok("if x { 1 } else { 2 }");
    match e {
        Expr::If(_, _, _, _) => {}
        _ => panic!("Expected if expression"),
    }
}

#[test]
fn test_match_expr() {
    let file = parse_ok("fn test() { match x { 0 => true, _ => false } }");
    match &file.items[0] {
        Item::Function(f) => match &f.body {
            Expr::Block(_, Some(e), _) => match e.as_ref() {
                Expr::Match(_, arms, _) => {
                    assert_eq!(arms.len(), 2);
                }
                _ => panic!("Expected match"),
            },
            _ => panic!("Expected block with match"),
        },
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_tuple() {
    let e = parse_expr_ok("(1, 2, 3)");
    match e {
        Expr::Tuple(elements, _) => {
            assert_eq!(elements.len(), 3);
        }
        _ => panic!("Expected tuple"),
    }
}

#[test]
fn test_unit() {
    let e = parse_expr_ok("()");
    match e {
        Expr::Unit(_) => {}
        _ => panic!("Expected unit"),
    }
}

#[test]
fn test_lambda_pipe() {
    let e = parse_expr_ok("|x| x + 1");
    match e {
        Expr::Lambda(params, _, _) => {
            assert_eq!(params.len(), 1);
        }
        _ => panic!("Expected lambda"),
    }
}

#[test]
fn test_lambda_fn() {
    let e = parse_expr_ok("fn(x: Nat) => x");
    match e {
        Expr::Lambda(params, _, _) => {
            assert_eq!(params.len(), 1);
            assert!(params[0].ty.is_some());
        }
        _ => panic!("Expected lambda"),
    }
}

#[test]
fn test_type_annotation() {
    let e = parse_expr_ok("x : Nat");
    match e {
        Expr::Annot(_, _, _) => {}
        _ => panic!("Expected annotation"),
    }
}

#[test]
fn test_sorry() {
    let e = parse_expr_ok("sorry");
    match e {
        Expr::Sorry(_) => {}
        _ => panic!("Expected sorry"),
    }
}

// Type tests

#[test]
fn test_arrow_type() {
    let file = parse_ok("fn test(f: Nat -> Bool) {}");
    match &file.items[0] {
        Item::Function(f) => match &f.params[0].ty {
            TypeExpr::Arrow(_, _, _) => {}
            _ => panic!("Expected arrow type"),
        },
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_product_type() {
    let file = parse_ok("fn test(p: Nat * Bool) {}");
    match &file.items[0] {
        Item::Function(f) => match &f.params[0].ty {
            TypeExpr::Product(_, _, _) => {}
            _ => panic!("Expected product type"),
        },
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_sum_type() {
    let file = parse_ok("fn test(s: Nat + Bool) {}");
    match &file.items[0] {
        Item::Function(f) => match &f.params[0].ty {
            TypeExpr::Sum(_, _, _) => {}
            _ => panic!("Expected sum type"),
        },
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_forall_type() {
    let file = parse_ok("fn test(f: forall T. T -> T) {}");
    match &file.items[0] {
        Item::Function(f) => match &f.params[0].ty {
            TypeExpr::Forall(_, _, _) => {}
            _ => panic!("Expected forall type"),
        },
        _ => panic!("Expected function"),
    }
}

// Pattern tests

#[test]
fn test_wildcard_pattern() {
    let file = parse_ok("fn test() { match x { _ => 0 } }");
    // Just check it parses
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

// Error recovery tests

#[test]
fn test_error_recovery() {
    let (file, errors) = parse("fn foo() {} @ fn bar() {}");
    // Should recover and parse both functions
    assert!(errors.len() >= 1);
    // At least foo should be parsed
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
    // Regression test: using a reserved keyword where an identifier is expected
    // should produce an error but not cause an infinite loop.
    let (file, errors) = parse("fn main() -> Nat { const(1, 2) }");
    // Should have errors for using reserved keyword
    assert!(!errors.is_empty());
    // Parser should still produce a result (with error recovery)
    assert!(!file.items.is_empty());
}

#[test]
fn test_reserved_keyword_as_function_name() {
    // Using a reserved keyword as a function name should error
    let (_file, errors) = parse("fn const() -> Nat { 42 }");
    assert!(!errors.is_empty());
    // Error should be about reserved keyword
    assert!(errors
        .iter()
        .any(|e| matches!(e.kind, ParseErrorKind::ReservedKeyword(_))));
}
// ─────────────────────────────────────────────────────────────────────────
// Record types tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_record_type_def() {
    let file = parse_ok("type Point = { x: Nat, y: Nat }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Point");
            match &t.body {
                TypeBody::Record(fields) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name.name, "x");
                    assert_eq!(fields[1].name.name, "y");
                }
                _ => panic!("Expected record type"),
            }
        }
        _ => panic!("Expected type def"),
    }
}

#[test]
fn test_record_type_single_field() {
    let file = parse_ok("type Wrapper = { inner: String }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Wrapper");
            match &t.body {
                TypeBody::Record(fields) => {
                    assert_eq!(fields.len(), 1);
                    assert_eq!(fields[0].name.name, "inner");
                }
                _ => panic!("Expected record type"),
            }
        }
        _ => panic!("Expected type def"),
    }
}

#[test]
fn test_record_type_trailing_comma() {
    let file = parse_ok("type Point = { x: Nat, y: Nat, }");
    match &file.items[0] {
        Item::TypeDef(t) => match &t.body {
            TypeBody::Record(fields) => {
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("Expected record type"),
        },
        _ => panic!("Expected type def"),
    }
}

#[test]
fn test_record_literal() {
    let expr = parse_expr_ok("{ x: 10, y: 20 }");
    match expr {
        Expr::RecordLit { spread, fields, .. } => {
            assert!(spread.is_none());
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0.name, "x");
            assert_eq!(fields[1].0.name, "y");
        }
        _ => panic!("Expected record literal"),
    }
}

#[test]
fn test_record_literal_single_field() {
    let expr = parse_expr_ok("{ inner: s }");
    match expr {
        Expr::RecordLit { spread, fields, .. } => {
            assert!(spread.is_none());
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].0.name, "inner");
        }
        _ => panic!("Expected record literal"),
    }
}

#[test]
fn test_record_literal_with_spread() {
    let expr = parse_expr_ok("{ ...base, x: 10 }");
    match expr {
        Expr::RecordLit { spread, fields, .. } => {
            assert!(spread.is_some());
            match spread.as_deref().unwrap() {
                Expr::Path(path) => {
                    assert!(path.is_simple());
                    assert_eq!(path.item_name().name, "base");
                }
                _ => panic!("Expected path as spread expression"),
            }
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].0.name, "x");
        }
        _ => panic!("Expected record literal"),
    }
}

#[test]
fn test_record_literal_spread_only() {
    // Spread with no explicit fields (copy)
    let expr = parse_expr_ok("{ ...p }");
    match expr {
        Expr::RecordLit { spread, fields, .. } => {
            assert!(spread.is_some());
            assert!(fields.is_empty());
        }
        _ => panic!("Expected record literal"),
    }
}

#[test]
fn test_record_literal_spread_multiple_fields() {
    let expr = parse_expr_ok("{ ...base, x: 1, y: 2 }");
    match expr {
        Expr::RecordLit { spread, fields, .. } => {
            assert!(spread.is_some());
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0.name, "x");
            assert_eq!(fields[1].0.name, "y");
        }
        _ => panic!("Expected record literal"),
    }
}

#[test]
fn test_record_literal_spread_with_field_access() {
    // Spread from a field access expression
    let expr = parse_expr_ok("{ ...r.inner, x: 5 }");
    match expr {
        Expr::RecordLit { spread, fields, .. } => {
            assert!(spread.is_some());
            match spread.as_deref().unwrap() {
                Expr::Field(_, field, _) => assert_eq!(field.name, "inner"),
                _ => panic!("Expected field access as spread"),
            }
            assert_eq!(fields.len(), 1);
        }
        _ => panic!("Expected record literal"),
    }
}

#[test]
fn test_block_expr_not_record() {
    // Block expressions should not be confused with records
    let expr = parse_expr_ok("{ let x = 1; x }");
    match expr {
        Expr::Block(_, _, _) => {}
        _ => panic!("Expected block expression"),
    }
}

#[test]
fn test_field_access() {
    let expr = parse_expr_ok("point.x");
    match expr {
        Expr::Field(base, field, _) => {
            match base.as_ref() {
                Expr::Path(path) => {
                    assert!(path.is_simple());
                    assert_eq!(path.item_name().name, "point");
                }
                _ => panic!("Expected path as base"),
            }
            assert_eq!(field.name, "x");
        }
        _ => panic!("Expected field access"),
    }
}

#[test]
fn test_chained_field_access() {
    let expr = parse_expr_ok("r.origin.x");
    match expr {
        Expr::Field(base, field, _) => {
            assert_eq!(field.name, "x");
            match base.as_ref() {
                Expr::Field(inner_base, inner_field, _) => {
                    assert_eq!(inner_field.name, "origin");
                    match inner_base.as_ref() {
                        Expr::Path(path) => {
                            assert!(path.is_simple());
                            assert_eq!(path.item_name().name, "r");
                        }
                        _ => panic!("Expected path as innermost base"),
                    }
                }
                _ => panic!("Expected field access as base"),
            }
        }
        _ => panic!("Expected field access"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Item Visibility Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_pub_function() {
    let file = parse_ok("pub fn public_fn() { 42 }");
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "public_fn");
            assert_eq!(f.visibility, Visibility::Public);
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_pub_crate_function() {
    let file = parse_ok("pub(crate) fn internal_fn() { 42 }");
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "internal_fn");
            assert_eq!(f.visibility, Visibility::Crate);
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_private_function() {
    let file = parse_ok("fn private_fn() { 42 }");
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "private_fn");
            assert_eq!(f.visibility, Visibility::Private);
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_pub_type_alias() {
    let file = parse_ok("pub type MyNat = Nat");
    match &file.items[0] {
        Item::TypeAlias(t) => {
            assert_eq!(t.name.name, "MyNat");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected type alias"),
    }
}

#[test]
fn test_pub_crate_type_alias() {
    let file = parse_ok("pub(crate) type InternalNat = Nat");
    match &file.items[0] {
        Item::TypeAlias(t) => {
            assert_eq!(t.name.name, "InternalNat");
            assert_eq!(t.visibility, Visibility::Crate);
        }
        _ => panic!("Expected type alias"),
    }
}

#[test]
fn test_pub_type_def() {
    let file = parse_ok("pub type Option<T> = None | Some(T)");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Option");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected type def"),
    }
}

#[test]
fn test_pub_crate_type_def() {
    let file = parse_ok("pub(crate) type Internal = A | B");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Internal");
            assert_eq!(t.visibility, Visibility::Crate);
        }
        _ => panic!("Expected type def"),
    }
}

#[test]
fn test_pub_theorem() {
    let file = parse_ok("pub theorem my_thm : Nat { 0 }");
    match &file.items[0] {
        Item::Theorem(t) => {
            assert_eq!(t.name.name, "my_thm");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected theorem"),
    }
}

#[test]
fn test_pub_crate_theorem() {
    let file = parse_ok("pub(crate) theorem internal_thm : Nat { 0 }");
    match &file.items[0] {
        Item::Theorem(t) => {
            assert_eq!(t.name.name, "internal_thm");
            assert_eq!(t.visibility, Visibility::Crate);
        }
        _ => panic!("Expected theorem"),
    }
}

#[test]
fn test_pub_lemma() {
    let file = parse_ok("pub lemma my_lemma : Nat { 0 }");
    match &file.items[0] {
        Item::Lemma(t) => {
            assert_eq!(t.name.name, "my_lemma");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected lemma"),
    }
}

#[test]
fn test_pub_axiom() {
    let file = parse_ok("pub axiom my_axiom : Nat");
    match &file.items[0] {
        Item::Axiom(a) => {
            assert_eq!(a.name.name, "my_axiom");
            assert_eq!(a.visibility, Visibility::Public);
        }
        _ => panic!("Expected axiom"),
    }
}

#[test]
fn test_pub_crate_axiom() {
    let file = parse_ok("pub(crate) axiom internal_axiom : Nat");
    match &file.items[0] {
        Item::Axiom(a) => {
            assert_eq!(a.name.name, "internal_axiom");
            assert_eq!(a.visibility, Visibility::Crate);
        }
        _ => panic!("Expected axiom"),
    }
}

#[test]
fn test_pub_extern_fn() {
    let file = parse_ok("pub extern fn print(s: String) -> Unit");
    match &file.items[0] {
        Item::ExternFn(e) => {
            assert_eq!(e.name.name, "print");
            assert_eq!(e.visibility, Visibility::Public);
        }
        _ => panic!("Expected extern fn"),
    }
}

#[test]
fn test_pub_crate_extern_fn() {
    let file = parse_ok("pub(crate) extern fn internal_print(s: String) -> Unit");
    match &file.items[0] {
        Item::ExternFn(e) => {
            assert_eq!(e.name.name, "internal_print");
            assert_eq!(e.visibility, Visibility::Crate);
        }
        _ => panic!("Expected extern fn"),
    }
}

#[test]
fn test_pub_struct() {
    let file = parse_ok("pub struct Point { x: Nat, y: Nat }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Point");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected struct"),
    }
}

#[test]
fn test_pub_crate_struct() {
    let file = parse_ok("pub(crate) struct InternalPoint { x: Nat }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "InternalPoint");
            assert_eq!(t.visibility, Visibility::Crate);
        }
        _ => panic!("Expected struct"),
    }
}

#[test]
fn test_pub_enum() {
    let file = parse_ok("pub enum Color { Red, Green, Blue }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Color");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected enum"),
    }
}

#[test]
fn test_pub_crate_enum() {
    let file = parse_ok("pub(crate) enum InternalColor { A, B }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "InternalColor");
            assert_eq!(t.visibility, Visibility::Crate);
        }
        _ => panic!("Expected enum"),
    }
}

#[test]
fn test_pub_crate_mod() {
    let file = parse_ok("pub(crate) mod internal;");
    match &file.items[0] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "internal");
            assert_eq!(m.visibility, Visibility::Crate);
        }
        _ => panic!("Expected mod"),
    }
}

#[test]
fn test_pub_crate_use() {
    let file = parse_ok("pub(crate) use internal::Helper;");
    match &file.items[0] {
        Item::Use(u) => {
            assert_eq!(u.visibility, Visibility::Crate);
        }
        _ => panic!("Expected use"),
    }
}

#[test]
fn test_mixed_visibility_items() {
    let file = parse_ok(
        r#"
        pub fn public_fn() { 1 }
        pub(crate) fn crate_fn() { 2 }
        fn private_fn() { 3 }
        pub type PublicType = Nat
        pub(crate) type CrateType = Bool
        type PrivateType = Unit
    "#,
    );
    assert_eq!(file.items.len(), 6);

    // Check public function
    match &file.items[0] {
        Item::Function(f) => assert_eq!(f.visibility, Visibility::Public),
        _ => panic!("Expected function"),
    }

    // Check crate function
    match &file.items[1] {
        Item::Function(f) => assert_eq!(f.visibility, Visibility::Crate),
        _ => panic!("Expected function"),
    }

    // Check private function
    match &file.items[2] {
        Item::Function(f) => assert_eq!(f.visibility, Visibility::Private),
        _ => panic!("Expected function"),
    }

    // Check public type alias
    match &file.items[3] {
        Item::TypeAlias(t) => assert_eq!(t.visibility, Visibility::Public),
        _ => panic!("Expected type alias"),
    }

    // Check crate type alias
    match &file.items[4] {
        Item::TypeAlias(t) => assert_eq!(t.visibility, Visibility::Crate),
        _ => panic!("Expected type alias"),
    }

    // Check private type alias
    match &file.items[5] {
        Item::TypeAlias(t) => assert_eq!(t.visibility, Visibility::Private),
        _ => panic!("Expected type alias"),
    }
}
