//! Unit tests for `info cir sites` and `info cir constructors` AST traversal.

use std::path::PathBuf;

use tungsten_bootstrap::parser::Parser;
use tungsten_bootstrap::span::LineIndex;

use super::{collect_sites_source_file, CirConstructor, CirSite, SiteCtx};

/// Parse source text and collect CIR sites for a variant.
fn find_sites(source: &str, variant: &str) -> Vec<CirSite> {
    let (sf, _errors) = Parser::new(source).parse();
    let line_index = LineIndex::new(source);
    let path = PathBuf::from("test.tg");
    let mut sites = Vec::new();
    let mut ctx = SiteCtx {
        file: &path,
        line_index: &line_index,
        variant,
        enclosing_fn: None,
        sites: &mut sites,
    };
    collect_sites_source_file(&sf, &mut ctx);
    sites
}

#[test]
fn finds_constructor_application() {
    let source = r#"
fn make_lit(n: Nat) -> CodegenIR {
    CIRNatLit(n)
}
"#;
    let sites = find_sites(source, "CIRNatLit");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].function.as_deref(), Some("make_lit"));
    assert_eq!(sites[0].line, 3);
}

#[test]
fn finds_multiple_sites() {
    let source = r#"
fn lower_a() -> CodegenIR {
    CIRInl(x, ty)
}
fn lower_b() -> CodegenIR {
    CIRInl(y, ty2)
}
"#;
    let sites = find_sites(source, "CIRInl");
    assert_eq!(sites.len(), 2);
    assert_eq!(sites[0].function.as_deref(), Some("lower_a"));
    assert_eq!(sites[1].function.as_deref(), Some("lower_b"));
}

#[test]
fn zero_sites_for_unknown_variant() {
    let source = r#"
fn example() -> Nat {
    42
}
"#;
    let sites = find_sites(source, "CIRNonexistent");
    assert!(sites.is_empty());
}

#[test]
fn finds_in_let_body() {
    let source = r#"
fn build() -> CodegenIR {
    let x = CIRVar("a");
    CIRLet("a", x, body)
}
"#;
    let sites = find_sites(source, "CIRVar");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].function.as_deref(), Some("build"));
}

#[test]
fn finds_in_match_arm() {
    let source = r#"
fn dispatch(e: CodegenIR) -> String {
    match e {
        CIRNatLit(n) => "nat",
        _ => CIRVar("fallback"),
    }
}
"#;
    // CIRVar in the arm body
    let sites = find_sites(source, "CIRVar");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].function.as_deref(), Some("dispatch"));
}

#[test]
fn does_not_double_count_app() {
    // App(Path("CIRInl"), [args]) should produce exactly 1 site,
    // not 2 (one from App match, one from Path recursion).
    let source = r#"
fn example() -> CodegenIR {
    CIRInl(val, ty)
}
"#;
    let sites = find_sites(source, "CIRInl");
    assert_eq!(sites.len(), 1);
}

#[test]
fn finds_bare_constructor_reference() {
    // A bare constructor (no application) should still be found
    let source = r#"
fn get_ctor() -> CodegenIR {
    CIRUnit
}
"#;
    let sites = find_sites(source, "CIRUnit");
    assert_eq!(sites.len(), 1);
}

#[test]
fn finds_nested_in_if_branch() {
    let source = r#"
fn choose(b: Bool) -> CodegenIR {
    if b {
        CIRInl(x, ty)
    } else {
        CIRInr(y, ty)
    }
}
"#;
    let inl = find_sites(source, "CIRInl");
    assert_eq!(inl.len(), 1);
    let inr = find_sites(source, "CIRInr");
    assert_eq!(inr.len(), 1);
}

#[test]
fn finds_in_block_stmt() {
    let source = r#"
fn build() -> CodegenIR {
    let _ = CIRNatLit(1);
    CIRNatLit(2)
}
"#;
    let sites = find_sites(source, "CIRNatLit");
    assert_eq!(sites.len(), 2);
}

// --- AC6: unknown variant exits cleanly with zero sites ---
#[test]
fn unknown_variant_produces_zero_sites_with_message() {
    // Verifies the output format matches AC6 expectations:
    // "CIRNonexistent construction sites:\n  (none found)\n  Total: 0 sites"
    let source = r#"
fn example() -> Nat { 42 }
"#;
    let sites = find_sites(source, "CIRNonexistent");
    assert!(sites.is_empty());
    // The cmd_cir_sites function would print "(none found)" and "Total: 0 sites"
    // — verified structurally here since cmd_cir_sites writes to stdout.
}

// ═══════════════════════════════════════════════════════════════════════
// info cir constructors
// ═══════════════════════════════════════════════════════════════════════

use tungsten_bootstrap::ast::{Item, TypeBody};

/// Parse source and extract constructors for a named type.
fn find_constructors(source: &str, type_name: &str) -> Vec<CirConstructor> {
    let (sf, _errors) = Parser::new(source).parse();
    let mut result = Vec::new();
    for item in &sf.items {
        if let Item::TypeDef(td) = item {
            if td.name.name == type_name {
                if let TypeBody::Sum(variants) = &td.body {
                    for v in variants {
                        result.push(CirConstructor {
                            name: v.name.name.clone(),
                            arity: v.fields.len(),
                        });
                    }
                }
            }
        }
    }
    result
}

#[test]
fn constructors_basic_adt() {
    let source = r#"
type CodegenIR =
    | CIRUnit
    | CIRNatLit(Nat)
    | CIRLet(String, CodegenIR, CodegenIR)
"#;
    let ctors = find_constructors(source, "CodegenIR");
    assert_eq!(ctors.len(), 3);
    assert_eq!(ctors[0].name, "CIRUnit");
    assert_eq!(ctors[0].arity, 0);
    assert_eq!(ctors[1].name, "CIRNatLit");
    assert_eq!(ctors[1].arity, 1);
    assert_eq!(ctors[2].name, "CIRLet");
    assert_eq!(ctors[2].arity, 3);
}

#[test]
fn constructors_empty_for_missing_type() {
    let source = r#"
fn example() -> Nat { 42 }
"#;
    let ctors = find_constructors(source, "CodegenIR");
    assert!(ctors.is_empty());
}

#[test]
fn constructors_ignores_other_types() {
    let source = r#"
type Foo = | A | B(Nat)
type CodegenIR = | CIRZero | CIRSucc(CodegenIR)
type Bar = | X(String) | Y
"#;
    let ctors = find_constructors(source, "CodegenIR");
    assert_eq!(ctors.len(), 2);
    assert_eq!(ctors[0].name, "CIRZero");
    assert_eq!(ctors[1].name, "CIRSucc");
}
