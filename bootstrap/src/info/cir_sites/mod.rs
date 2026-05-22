//! `tungsten info cir` — CIR inspection commands.
//!
//! Subcommands:
//!   `sites`        — find CIR constructor application sites (cost 2).
//!   `constructors` — list all `CodegenIR` constructors with arities (cost 2).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tungsten_bootstrap::ast::{
    Expr, IfLetCondition, Item, MatchArm, Motive, SourceFile, Stmt, TypeBody,
};
use tungsten_bootstrap::driver::{parse_module_tree, ParsedModule};
use tungsten_bootstrap::span::LineIndex;
use tungsten_bootstrap::Spanned;

/// A single CIR constructor site found during AST traversal.
#[derive(Debug, Clone)]
pub(crate) struct CirSite {
    /// Source file path (relative when possible)
    pub(crate) file: PathBuf,
    /// 1-based line number
    pub(crate) line: u32,
    /// Name of the enclosing function (if any)
    pub(crate) function: Option<String>,
}

/// Traversal context threaded through all site-collection helpers.
struct SiteCtx<'a> {
    file: &'a Path,
    line_index: &'a LineIndex,
    variant: &'a str,
    enclosing_fn: Option<&'a str>,
    sites: &'a mut Vec<CirSite>,
}

impl SiteCtx<'_> {
    fn record(&mut self, span_start: u32) {
        let loc = self.line_index.location(span_start);
        self.sites.push(CirSite {
            file: self.file.to_path_buf(),
            line: loc.line,
            function: self.enclosing_fn.map(String::from),
        });
    }

    fn with_fn<'b>(&'b mut self, name: &'b str) -> SiteCtx<'b> {
        SiteCtx {
            file: self.file,
            line_index: self.line_index,
            variant: self.variant,
            enclosing_fn: Some(name),
            sites: self.sites,
        }
    }
}

/// Entry point for `tungsten info cir sites <variant> <file>`.
pub(crate) fn cmd_cir_sites(variant: &str, file: &Path) -> ExitCode {
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let module = match parse_module_tree(file, &mut visited, &mut chain, None) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let sites = collect_sites(&module, variant);

    if sites.is_empty() {
        println!("{variant} construction sites:");
        println!("  (none found)");
        println!("  Total: 0 sites");
        println!();
        println!(
            "Hint: verify the variant name is spelled exactly as it appears in the ADT definition."
        );
        return ExitCode::SUCCESS;
    }

    println!("{variant} construction sites:");
    for site in &sites {
        let fn_label = site
            .function
            .as_deref()
            .map_or(String::new(), |f| format!("  in {f}()"));
        println!("  {}:{}{fn_label}", site.file.display(), site.line);
    }
    println!("  Total: {} sites", sites.len());

    ExitCode::SUCCESS
}

/// Recursively collect CIR sites from a parsed module tree.
pub(crate) fn collect_sites(module: &ParsedModule, variant: &str) -> Vec<CirSite> {
    let mut sites = Vec::new();
    collect_sites_module(module, variant, &mut sites);
    sites
}

fn collect_sites_module(module: &ParsedModule, variant: &str, sites: &mut Vec<CirSite>) {
    let source = fs::read_to_string(&module.path).unwrap_or_default();
    let line_index = LineIndex::new(&source);

    let mut ctx = SiteCtx {
        file: &module.path,
        line_index: &line_index,
        variant,
        enclosing_fn: None,
        sites,
    };
    collect_sites_source_file(&module.source_file, &mut ctx);

    for sub in &module.submodules {
        collect_sites_module(sub, ctx.variant, ctx.sites);
    }
}

fn collect_sites_source_file(sf: &SourceFile, ctx: &mut SiteCtx) {
    for item in &sf.items {
        collect_sites_item(item, ctx);
    }
}

fn collect_sites_item(item: &Item, ctx: &mut SiteCtx) {
    match item {
        Item::Function(f) => {
            let fn_name = &f.name.name;
            collect_sites_expr(&f.body, &mut ctx.with_fn(fn_name));
        }
        Item::Theorem(t) | Item::Lemma(t) => {
            let fn_name = &t.name.name;
            collect_sites_expr(&t.body, &mut ctx.with_fn(fn_name));
        }
        _ => {}
    }
}

fn collect_sites_expr(expr: &Expr, ctx: &mut SiteCtx) {
    match expr {
        Expr::App(callee, args, _span) => {
            if expr_matches_variant(callee, ctx.variant) {
                ctx.record(callee.span().start);
            } else {
                collect_sites_expr(callee, ctx);
            }
            for arg in args {
                collect_sites_expr(arg, ctx);
            }
        }
        Expr::Path(p) => {
            if p.item_name().name == ctx.variant {
                ctx.record(p.span.start);
            }
        }
        Expr::Lambda(_, body, _) => collect_sites_expr(body, ctx),
        Expr::Binary(lhs, _, rhs, _) => {
            collect_sites_expr(lhs, ctx);
            collect_sites_expr(rhs, ctx);
        }
        Expr::Unary(_, e, _) | Expr::Paren(e, _) | Expr::Try(e, _) | Expr::TryBlock(e, _) => {
            collect_sites_expr(e, ctx);
        }
        Expr::Let(_, _, init, body, _) => {
            collect_sites_expr(init, ctx);
            collect_sites_expr(body, ctx);
        }
        Expr::LetElse(_, _, init, else_body, body, _) => {
            collect_sites_expr(init, ctx);
            collect_sites_expr(else_body, ctx);
            collect_sites_expr(body, ctx);
        }
        Expr::If(cond, then_br, else_br, _) => {
            collect_sites_expr(cond, ctx);
            collect_sites_expr(then_br, ctx);
            collect_sites_expr(else_br, ctx);
        }
        Expr::Match(scrutinee, arms, _) => {
            collect_sites_expr(scrutinee, ctx);
            for arm in arms {
                collect_sites_match_arm(arm, ctx);
            }
        }
        Expr::Block(stmts, tail, _) => {
            stmts.iter().for_each(|s| collect_sites_stmt(s, ctx));
            if let Some(tail) = tail {
                collect_sites_expr(tail, ctx);
            }
        }
        Expr::Tuple(elems, _) => {
            elems.iter().for_each(|e| collect_sites_expr(e, ctx));
        }
        Expr::RecordLit { spread, fields, .. } | Expr::NamedRecord { spread, fields, .. } => {
            if let Some(s) = spread {
                collect_sites_expr(s, ctx);
            }
            for (_, val) in fields {
                collect_sites_expr(val, ctx);
            }
        }
        Expr::Field(base, _, _) | Expr::TypeApp(base, _, _) | Expr::Annot(base, _, _) => {
            collect_sites_expr(base, ctx);
        }
        Expr::Return(Some(e), _) => collect_sites_expr(e, ctx),
        Expr::Have(_, _, proof, body, _) => {
            collect_sites_expr(proof, ctx);
            collect_sites_expr(body, ctx);
        }
        Expr::Subst(proof, motive, witness, _) => {
            collect_sites_expr(proof, ctx);
            if let Motive::Expr(e) = motive {
                collect_sites_expr(e, ctx);
            }
            collect_sites_expr(witness, ctx);
        }
        Expr::Sym(proof, _) => collect_sites_expr(proof, ctx),
        Expr::Trans(h1, h2, _) => {
            collect_sites_expr(h1, ctx);
            collect_sites_expr(h2, ctx);
        }
        Expr::Cong(f, proof, _) => {
            collect_sites_expr(f, ctx);
            collect_sites_expr(proof, ctx);
        }
        Expr::NatInd(motive, base, step, n, _) => {
            if let Motive::Expr(e) = motive {
                collect_sites_expr(e, ctx);
            }
            collect_sites_expr(base, ctx);
            collect_sites_expr(step, ctx);
            collect_sites_expr(n, ctx);
        }
        Expr::NatRec(_, base, step, n, _) => {
            collect_sites_expr(base, ctx);
            collect_sites_expr(step, ctx);
            collect_sites_expr(n, ctx);
        }
        Expr::Show(_, proof, _) => collect_sites_expr(proof, ctx),
        Expr::Assume(_, _, body, _) => collect_sites_expr(body, ctx),
        Expr::IfLet(_, init, body, else_branch, _) => {
            collect_sites_expr(init, ctx);
            collect_sites_expr(body, ctx);
            if let Some(e) = else_branch {
                collect_sites_expr(e, ctx);
            }
        }
        Expr::IfLetChain(conditions, body, else_branch, _) => {
            for cond in conditions {
                match cond {
                    IfLetCondition::Bind(_, init) => collect_sites_expr(init, ctx),
                    IfLetCondition::Guard(guard) => collect_sites_expr(guard, ctx),
                }
            }
            collect_sites_expr(body, ctx);
            if let Some(e) = else_branch {
                collect_sites_expr(e, ctx);
            }
        }
        // Leaf nodes
        Expr::IntLiteral(..)
        | Expr::BoolLiteral(..)
        | Expr::StringLiteral(..)
        | Expr::Unit(..)
        | Expr::Refl(..)
        | Expr::Sorry(..)
        | Expr::Return(None, _)
        | Expr::Error(..) => {}
    }
}

fn collect_sites_stmt(stmt: &Stmt, ctx: &mut SiteCtx) {
    match stmt {
        Stmt::Let(_, _, init, _) => collect_sites_expr(init, ctx),
        Stmt::LetElse(_, _, init, else_body, _) => {
            collect_sites_expr(init, ctx);
            collect_sites_expr(else_body, ctx);
        }
        Stmt::Expr(e, _) => collect_sites_expr(e, ctx),
        Stmt::Item(item) => collect_sites_item(item, ctx),
    }
}

fn collect_sites_match_arm(arm: &MatchArm, ctx: &mut SiteCtx) {
    if let Some(guard) = &arm.guard {
        collect_sites_expr(guard, ctx);
    }
    collect_sites_expr(&arm.body, ctx);
}

/// Check if an expression is a path reference to the given variant name.
fn expr_matches_variant(expr: &Expr, variant: &str) -> bool {
    match expr {
        Expr::Path(p) => p.item_name().name == variant,
        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// tungsten info cir constructors
// ═══════════════════════════════════════════════════════════════════════

/// A CIR constructor extracted from the `CodegenIR` ADT.
#[derive(Debug, Clone)]
pub(crate) struct CirConstructor {
    /// Variant name (e.g., "`CIRNatLit`")
    pub(crate) name: String,
    /// Number of fields (arity)
    pub(crate) arity: usize,
}

/// Entry point for `tungsten info cir constructors <file>`.
pub(crate) fn cmd_cir_constructors(file: &Path) -> ExitCode {
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let module = match parse_module_tree(file, &mut visited, &mut chain, None) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let ctors = collect_constructors(&module, "CodegenIR");

    if ctors.is_empty() {
        eprintln!("error: CodegenIR type not found in the module tree");
        eprintln!("Hint: ensure the file imports or defines the CIR types module.");
        return ExitCode::FAILURE;
    }

    println!("CodegenIR constructors ({} variants):", ctors.len());
    for ctor in &ctors {
        println!("  {} ({})", ctor.name, ctor.arity);
    }

    ExitCode::SUCCESS
}

/// Recursively search the parsed module tree for a type definition
/// and extract its constructors.
pub(crate) fn collect_constructors(module: &ParsedModule, type_name: &str) -> Vec<CirConstructor> {
    let mut result = Vec::new();
    collect_constructors_module(module, type_name, &mut result);
    result
}

fn collect_constructors_module(
    module: &ParsedModule,
    type_name: &str,
    result: &mut Vec<CirConstructor>,
) {
    // Only take the first match — the type is defined in exactly one module.
    if !result.is_empty() {
        return;
    }

    for item in &module.source_file.items {
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
                return;
            }
        }
    }

    for sub in &module.submodules {
        collect_constructors_module(sub, type_name, result);
    }
}

#[cfg(test)]
mod tests;
