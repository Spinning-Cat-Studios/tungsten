//! `tungsten doctor check nested-patterns` — detect nested constructor+tuple match patterns.
//!
//! Walks all match arms in the AST and reports patterns of the form `Ctor((a, b))` where
//! a constructor pattern contains a tuple subpattern. These patterns are known to cause
//! "unknown value" errors in the self-compiled binary (tungsten1) due to a binding-scope
//! propagation bug (ADR 20.5.26a).
//!
//! Cost 2: parse only, no elaboration required.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::ast::{Expr, Item, Motive, Pattern, Stmt};
use crate::driver::modules::parse::parse_module_tree;
use crate::driver::modules::ParsedModule;

/// A detected nested constructor+tuple pattern site.
struct NestedPatternSite {
    /// File path where the pattern appears
    file: String,
    /// Function or definition name containing the pattern
    def_name: String,
    /// Byte offset (for map-span lookup)
    offset: u32,
    /// Description of the pattern shape
    shape: String,
}

/// Entry point for `tungsten doctor check nested-patterns <file>`.
pub fn cmd_check_nested_patterns(file: &PathBuf, verbose: bool) -> ExitCode {
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let module_tree = match parse_module_tree(file, &mut visited, &mut chain, None) {
        Ok(tree) => tree,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut sites = Vec::new();
    collect_from_parsed_module(&module_tree, &mut sites);

    if sites.is_empty() {
        println!(
            "✓ No nested constructor+tuple patterns found in {}",
            file.display()
        );
        ExitCode::SUCCESS
    } else {
        println!(
            "⚠ {} nested constructor+tuple pattern(s) found (ADR 20.5.26a):\n",
            sites.len()
        );
        for site in &sites {
            println!(
                "  {}  offset {}  in {}",
                site.file, site.offset, site.def_name
            );
            if verbose {
                println!("    pattern: {}", site.shape);
            }
        }
        println!(
            "\nThese patterns may cause \"unknown value\" errors in tungsten1.\n\
             Workaround: split into two-step matches:\n\
             \n\
             Before:  Ok((a, b)) => a + b\n\
             After:   Ok(pair) => let a = pair.0; let b = pair.1; a + b\n\
             \n\
             Use `tungsten doctor map-span <file> <offset>` for file:line:col.\n\
             See ADR 20.5.26a for details."
        );
        ExitCode::from(2) // Distinguish from hard failure (1)
    }
}

/// Recursively collect nested pattern sites from the parsed module tree.
fn collect_from_parsed_module(module: &ParsedModule, sites: &mut Vec<NestedPatternSite>) {
    let file_path = module.path.display().to_string();

    for item in &module.source_file.items {
        collect_from_item(item, &file_path, sites);
    }

    for child in &module.submodules {
        collect_from_parsed_module(child, sites);
    }
}

/// Collect nested pattern sites from a single item.
fn collect_from_item(item: &Item, file: &str, sites: &mut Vec<NestedPatternSite>) {
    match item {
        Item::Function(f) => {
            collect_from_expr(&f.body, file, &f.name.name, sites);
        }
        Item::Theorem(t) | Item::Lemma(t) => {
            collect_from_expr(&t.body, file, &t.name.name, sites);
        }
        _ => {}
    }
}

/// Walk an expression tree, looking for match expressions with nested patterns.
fn collect_from_expr(expr: &Expr, file: &str, def_name: &str, sites: &mut Vec<NestedPatternSite>) {
    match expr {
        Expr::Match(scrutinee, arms, _) => {
            collect_from_expr(scrutinee, file, def_name, sites);
            for arm in arms {
                check_pattern_for_nesting(&arm.pattern, file, def_name, sites);
                if let Some(guard) = &arm.guard {
                    collect_from_expr(guard, file, def_name, sites);
                }
                collect_from_expr(&arm.body, file, def_name, sites);
            }
        }
        Expr::App(f, args, _) => {
            collect_from_expr(f, file, def_name, sites);
            for arg in args {
                collect_from_expr(arg, file, def_name, sites);
            }
        }
        Expr::Lambda(_, body, _) => {
            collect_from_expr(body, file, def_name, sites);
        }
        Expr::Let(_, _, value, body, _) => {
            collect_from_expr(value, file, def_name, sites);
            collect_from_expr(body, file, def_name, sites);
        }
        Expr::LetElse(_, _, value, else_br, body, _) => {
            collect_from_expr(value, file, def_name, sites);
            collect_from_expr(else_br, file, def_name, sites);
            collect_from_expr(body, file, def_name, sites);
        }
        Expr::If(cond, then_br, else_br, _) => {
            collect_from_expr(cond, file, def_name, sites);
            collect_from_expr(then_br, file, def_name, sites);
            collect_from_expr(else_br, file, def_name, sites);
        }
        Expr::IfLet(_, scrutinee, then_br, else_br, _) => {
            collect_from_expr(scrutinee, file, def_name, sites);
            collect_from_expr(then_br, file, def_name, sites);
            if let Some(e) = else_br {
                collect_from_expr(e, file, def_name, sites);
            }
        }
        Expr::Block(stmts, trailing, _) => {
            for stmt in stmts {
                match stmt {
                    Stmt::Expr(e, _) => collect_from_expr(e, file, def_name, sites),
                    Stmt::Let(_, _, value, _) => {
                        collect_from_expr(value, file, def_name, sites);
                    }
                    Stmt::LetElse(_, _, value, else_br, _) => {
                        collect_from_expr(value, file, def_name, sites);
                        collect_from_expr(else_br, file, def_name, sites);
                    }
                    Stmt::Item(item) => collect_from_item(item, file, sites),
                }
            }
            if let Some(e) = trailing {
                collect_from_expr(e, file, def_name, sites);
            }
        }
        Expr::Tuple(elems, _) => {
            for e in elems {
                collect_from_expr(e, file, def_name, sites);
            }
        }
        Expr::Field(base, _, _) => {
            collect_from_expr(base, file, def_name, sites);
        }
        Expr::Try(inner, _) | Expr::TryBlock(inner, _) | Expr::Paren(inner, _) => {
            collect_from_expr(inner, file, def_name, sites);
        }
        Expr::Return(inner, _) => {
            if let Some(e) = inner {
                collect_from_expr(e, file, def_name, sites);
            }
        }
        Expr::Binary(lhs, _, rhs, _) => {
            collect_from_expr(lhs, file, def_name, sites);
            collect_from_expr(rhs, file, def_name, sites);
        }
        Expr::Unary(_, inner, _) => {
            collect_from_expr(inner, file, def_name, sites);
        }
        Expr::Annot(inner, _, _) | Expr::TypeApp(inner, _, _) => {
            collect_from_expr(inner, file, def_name, sites);
        }
        Expr::Have(_, _, proof, body, _) => {
            collect_from_expr(proof, file, def_name, sites);
            collect_from_expr(body, file, def_name, sites);
        }
        Expr::Subst(proof, motive, witness, _) => {
            collect_from_expr(proof, file, def_name, sites);
            if let Motive::Expr(e) = motive {
                collect_from_expr(e, file, def_name, sites);
            }
            collect_from_expr(witness, file, def_name, sites);
        }
        Expr::Sym(proof, _) => {
            collect_from_expr(proof, file, def_name, sites);
        }
        Expr::Trans(h1, h2, _) => {
            collect_from_expr(h1, file, def_name, sites);
            collect_from_expr(h2, file, def_name, sites);
        }
        Expr::Cong(f, proof, _) => {
            collect_from_expr(f, file, def_name, sites);
            collect_from_expr(proof, file, def_name, sites);
        }
        Expr::NatInd(motive, base, step, n, _) => {
            if let Motive::Expr(e) = motive {
                collect_from_expr(e, file, def_name, sites);
            }
            collect_from_expr(base, file, def_name, sites);
            collect_from_expr(step, file, def_name, sites);
            collect_from_expr(n, file, def_name, sites);
        }
        Expr::NatRec(_, base, step, n, _) => {
            collect_from_expr(base, file, def_name, sites);
            collect_from_expr(step, file, def_name, sites);
            collect_from_expr(n, file, def_name, sites);
        }
        Expr::Assume(_, _, proof, _) => {
            collect_from_expr(proof, file, def_name, sites);
        }
        Expr::Show(_, proof, _) => {
            collect_from_expr(proof, file, def_name, sites);
        }
        Expr::RecordLit { spread, fields, .. } | Expr::NamedRecord { spread, fields, .. } => {
            if let Some(s) = spread {
                collect_from_expr(s, file, def_name, sites);
            }
            for (_, v) in fields {
                collect_from_expr(v, file, def_name, sites);
            }
        }
        Expr::IfLetChain(_, then_br, else_br, _) => {
            collect_from_expr(then_br, file, def_name, sites);
            if let Some(e) = else_br {
                collect_from_expr(e, file, def_name, sites);
            }
        }
        // Leaf expressions — no sub-expressions to walk
        Expr::Path(_)
        | Expr::IntLiteral(_, _)
        | Expr::BoolLiteral(_, _)
        | Expr::StringLiteral(_, _)
        | Expr::Unit(_)
        | Expr::Refl(_)
        | Expr::Sorry(_)
        | Expr::Error(_) => {}
    }
}

/// Check if a pattern has a nested constructor+tuple shape.
fn check_pattern_for_nesting(
    pattern: &Pattern,
    file: &str,
    def_name: &str,
    sites: &mut Vec<NestedPatternSite>,
) {
    match pattern {
        Pattern::Constructor(path, sub_patterns, span) => {
            let ctor_name = &path.item_name().name;
            for sub in sub_patterns {
                if matches!(sub, Pattern::Tuple(_, _)) {
                    sites.push(NestedPatternSite {
                        file: file.to_string(),
                        def_name: def_name.to_string(),
                        offset: span.start,
                        shape: format!("{}((<tuple>))", ctor_name),
                    });
                }
                // Recurse into sub-patterns for deeper nesting
                check_pattern_for_nesting(sub, file, def_name, sites);
            }
        }
        Pattern::Or(a, b, _) => {
            check_pattern_for_nesting(a, file, def_name, sites);
            check_pattern_for_nesting(b, file, def_name, sites);
        }
        Pattern::Tuple(sub_patterns, _) => {
            for sub in sub_patterns {
                check_pattern_for_nesting(sub, file, def_name, sites);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Ident, Path};
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 1)
    }

    fn var_pat(name: &str) -> Pattern {
        Pattern::Var(Ident {
            name: name.to_string(),
            span: dummy_span(),
        })
    }

    fn ctor_path(name: &str) -> Path {
        Path {
            segments: vec![Ident {
                name: name.to_string(),
                span: dummy_span(),
            }],
            span: dummy_span(),
        }
    }

    #[test]
    fn detects_ctor_tuple_pattern() {
        let pattern = Pattern::Constructor(
            ctor_path("Ok"),
            vec![Pattern::Tuple(
                vec![var_pat("a"), var_pat("b")],
                dummy_span(),
            )],
            dummy_span(),
        );
        let mut sites = Vec::new();
        check_pattern_for_nesting(&pattern, "test.tg", "test_fn", &mut sites);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].shape, "Ok((<tuple>))");
    }

    #[test]
    fn ignores_ctor_var_pattern() {
        let pattern = Pattern::Constructor(ctor_path("Ok"), vec![var_pat("x")], dummy_span());
        let mut sites = Vec::new();
        check_pattern_for_nesting(&pattern, "test.tg", "test_fn", &mut sites);
        assert!(sites.is_empty());
    }

    #[test]
    fn ignores_simple_tuple_pattern() {
        let pattern = Pattern::Tuple(vec![var_pat("a"), var_pat("b")], dummy_span());
        let mut sites = Vec::new();
        check_pattern_for_nesting(&pattern, "test.tg", "test_fn", &mut sites);
        assert!(sites.is_empty());
    }

    #[test]
    fn detects_nested_ctor_in_or_pattern() {
        let inner = Pattern::Constructor(
            ctor_path("Some"),
            vec![Pattern::Tuple(
                vec![var_pat("a"), var_pat("b")],
                dummy_span(),
            )],
            dummy_span(),
        );
        let pattern = Pattern::Or(
            Box::new(inner),
            Box::new(Pattern::Wildcard(dummy_span())),
            dummy_span(),
        );
        let mut sites = Vec::new();
        check_pattern_for_nesting(&pattern, "test.tg", "test_fn", &mut sites);
        assert_eq!(sites.len(), 1);
    }
}
