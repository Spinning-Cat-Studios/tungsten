#[cfg(test)]
mod tests {
    use crate::driver::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_run_file_single_module() {
        let dir = TempDir::new().unwrap();
        let main_path = dir.path().join("main.tg");
        fs::write(&main_path, "fn hello() -> Nat { 42 }").unwrap();

        let result = run_file(&main_path, Mode::Check, false).unwrap();
        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 1, .. }
        ));
    }

    #[test]
    fn test_run_file_with_submodule() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("main.tg"),
            "mod foo;\nfn main() -> Nat { helper() }",
        )
        .unwrap();

        fs::write(dir.path().join("foo.tg"), "pub fn helper() -> Nat { 42 }").unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false).unwrap();

        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 2, .. }
        ));
    }

    #[test]
    fn test_run_file_nested_modules() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("main.tg"),
            "mod math;\nfn main() -> Nat { add(1, 2) }",
        )
        .unwrap();

        let math_dir = dir.path().join("math");
        fs::create_dir(&math_dir).unwrap();

        fs::write(
            math_dir.join("mod.tg"),
            "mod ops;\nfn unused() -> Nat { 0 }",
        )
        .unwrap();

        fs::write(
            math_dir.join("ops.tg"),
            "pub fn add(a: Nat, b: Nat) -> Nat { a + b }",
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false).unwrap();

        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 3, .. }
        ));
    }

    #[test]
    fn test_run_file_module_not_found() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.tg"), "mod nonexistent;").unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false);

        assert!(matches!(result, Err(PipelineError::ModuleNotFound { .. })));
    }

    #[test]
    fn test_run_file_use_statement_cross_module() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("main.tg"),
            r#"
mod foo;
use foo::MyType;

fn make_my_type() -> MyType { MyType::A }
"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("foo.tg"),
            r#"
pub type MyType = A | B
"#,
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false).unwrap();

        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 1, .. }
        ));
    }

    #[test]
    fn test_run_file_use_statement_value_import() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("main.tg"),
            r#"
mod foo;
use foo::helper;

fn main() -> Nat { helper() }
"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("foo.tg"),
            r#"
pub fn helper() -> Nat { 42 }
"#,
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false).unwrap();

        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 2, .. }
        ));
    }

    #[test]
    fn test_run_file_use_statement_module_scoped() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("main.tg"),
            r#"
mod foo;
pub mod bar;

// This should fail because MyType is not imported here (only in foo.tg)
// For now, test that foo.tg's import works
fn main() -> Nat { 0 }
"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("bar.tg"),
            r#"
pub type MyType = A | B
"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("foo.tg"),
            r#"
use bar::MyType;

fn use_type() -> MyType { MyType::A }
"#,
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false).unwrap();

        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 2, .. }
        ));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Error file path tracking tests (ADR 4.1: Better Error Messages)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_error_in_submodule_has_correct_file_path() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("main.tg"),
            "mod foo;\nfn main() -> Nat { 0 }",
        )
        .unwrap();

        fs::write(
            dir.path().join("foo.tg"),
            r#"
pub fn broken() -> Nat {
    true  // Type mismatch: expected Nat, found Bool
}
"#,
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false);

        assert!(matches!(result, Ok(PipelineResult::Failed)));
    }

    #[test]
    fn test_error_in_nested_submodule_has_correct_file_path() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("main.tg"),
            "mod foo;\nfn main() -> Nat { 0 }",
        )
        .unwrap();

        let foo_dir = dir.path().join("foo");
        fs::create_dir(&foo_dir).unwrap();
        fs::write(
            foo_dir.join("mod.tg"),
            "mod bar;\npub fn foo_ok() -> Nat { 0 }",
        )
        .unwrap();

        fs::write(
            foo_dir.join("bar.tg"),
            r#"
pub fn bar_broken() -> Bool {
    42  // Type mismatch: expected Bool, found Nat
}
"#,
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false);

        assert!(matches!(result, Ok(PipelineResult::Failed)));
    }

    #[test]
    fn test_cache_disabled_reason_both() {
        assert_eq!(
            super::super::cache_disabled_reason(true, true),
            "[cache] disabled by --no-cache flag and TUNGSTEN_NO_CACHE env var"
        );
    }

    #[test]
    fn test_cache_disabled_reason_flag_only() {
        assert_eq!(
            super::super::cache_disabled_reason(true, false),
            "[cache] disabled by --no-cache flag"
        );
    }

    #[test]
    fn test_cache_disabled_reason_env_only() {
        assert_eq!(
            super::super::cache_disabled_reason(false, true),
            "[cache] disabled by TUNGSTEN_NO_CACHE env var"
        );
    }

    #[test]
    fn test_cache_disabled_reason_neither() {
        // When neither flag is set, the env-only message is the fallback
        // (caller only invokes this when skip_cache is true)
        assert_eq!(
            super::super::cache_disabled_reason(false, false),
            "[cache] disabled by TUNGSTEN_NO_CACHE env var"
        );
    }

    #[test]
    fn test_build_codegen_units_per_file() {
        use crate::elaborate::CoreDef;
        use crate::span::Span;
        use std::path::PathBuf;
        use tungsten_core::terms::SpannedTerm;
        use tungsten_core::{Term, Type};

        let make_def = |name: &str| CoreDef {
            name: name.to_string(),
            ty: Type::Nat,
            term: SpannedTerm {
                term: Term::Zero,
                span: None,
            },
            span: Span::new(0, 0),
        };

        let module_defs = vec![
            // Root-level defs (main.tg)
            (
                vec![],
                PathBuf::from("/project/src/main.tg"),
                vec![make_def("main")],
            ),
            // Top-level module "lexer"
            (
                vec!["lexer".into()],
                PathBuf::from("/project/src/lexer.tg"),
                vec![make_def("scan")],
            ),
            // Nested module "parser::ast" — separate unit (not folded)
            (
                vec!["parser".into(), "ast".into()],
                PathBuf::from("/project/src/parser/ast.tg"),
                vec![make_def("node")],
            ),
            // Top-level module "parser"
            (
                vec!["parser".into()],
                PathBuf::from("/project/src/parser/mod.tg"),
                vec![make_def("parse")],
            ),
        ];

        let units = super::super::output::build_codegen_units(module_defs);

        // Per-file: 4 separate units (no folding)
        assert_eq!(units.len(), 4);

        // Root unit
        let root = units.iter().find(|u| u.module_path.is_empty()).unwrap();
        assert_eq!(root.defs.len(), 1);
        assert_eq!(root.defs[0].name, "main");
        assert_eq!(root.source_file, PathBuf::from("/project/src/main.tg"));

        // Lexer unit
        let lexer = units
            .iter()
            .find(|u| u.module_path == vec!["lexer"])
            .unwrap();
        assert_eq!(lexer.defs.len(), 1);
        assert_eq!(lexer.defs[0].name, "scan");

        // parser::ast is separate from parser (not folded)
        let ast = units
            .iter()
            .find(|u| u.module_path == vec!["parser", "ast"])
            .unwrap();
        assert_eq!(ast.defs.len(), 1);
        assert_eq!(ast.defs[0].name, "node");
        assert_eq!(ast.source_file, PathBuf::from("/project/src/parser/ast.tg"));

        // Parser unit
        let parser = units
            .iter()
            .find(|u| u.module_path == vec!["parser"] && u.defs[0].name == "parse")
            .unwrap();
        assert_eq!(parser.defs.len(), 1);
        assert_eq!(parser.defs[0].name, "parse");
    }

    #[test]
    fn test_build_codegen_units_filters_empty() {
        use crate::elaborate::CoreDef;
        use crate::span::Span;
        use std::path::PathBuf;
        use tungsten_core::terms::SpannedTerm;
        use tungsten_core::{Term, Type};

        let make_def = |name: &str| CoreDef {
            name: name.to_string(),
            ty: Type::Nat,
            term: SpannedTerm {
                term: Term::Zero,
                span: None,
            },
            span: Span::new(0, 0),
        };

        let module_defs = vec![
            (vec![], PathBuf::from("/p/main.tg"), vec![make_def("main")]),
            // Empty defs (type-only module) — should be filtered out
            (vec!["types".into()], PathBuf::from("/p/types.tg"), vec![]),
        ];

        let units = super::super::output::build_codegen_units(module_defs);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].defs[0].name, "main");
    }

    /// The IR cache's hit path, and its clean-collection guard (ADR 14.8.26g
    /// D2a): a second identical elaboration is served from cache, and a
    /// source whose collection pass deferred an error never is — a stale hit
    /// there would mask the failure entirely.
    #[test]
    fn an_ir_cache_hit_requires_a_clean_collection() {
        use crate::driver::modules::{ModuleInfo, SourceMap};
        use crate::driver::output::TraceOptions;
        use std::sync::Mutex;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("m.tg");
        let source = "fn forty_two() -> Nat { 42 }";
        fs::write(&path, source).unwrap();
        let (ast, parse_errors) = crate::parse(source);
        assert!(parse_errors.is_empty());

        let cache = Mutex::new(crate::cache::BuildCache::new(dir.path(), false).unwrap());
        // Seed the AST-manifest entry: `put_ir` refuses a file the AST cache
        // has never seen, and without it the "hit" leg would silently test a
        // permanent miss.
        cache.lock().unwrap().put(&path, source, &ast).unwrap();
        let build = crate::driver::pipeline::BuildCtx {
            cache: Some(&cache),
            module_info: ModuleInfo::default(),
            source_map: SourceMap::single(path.clone(), source.to_string()),
        };
        let trace = TraceOptions::default();

        let first = crate::driver::pipeline::ir_cached_elab::elaborate_with_ir_cache(
            &ast, &path, false, &build, &trace,
        )
        .expect("clean source must elaborate");
        let second = crate::driver::pipeline::ir_cached_elab::elaborate_with_ir_cache(
            &ast, &path, false, &build, &trace,
        )
        .expect("the cache hit must serve the same defs");
        assert_eq!(second.defs.len(), first.defs.len());

        // A deferred collection error must fail even when a matching cache
        // entry EXISTS: the broken source shares the clean run's path and (no
        // type definitions) its types hash, so without the clean-collection
        // guard the stale hit would return the clean run's defs and mask the
        // failure entirely.
        let bad_source = "fn f() -> NoSuchType { 0 }";
        fs::write(&path, bad_source).unwrap();
        let (bad_ast, _) = crate::parse(bad_source);
        let result = crate::driver::pipeline::ir_cached_elab::elaborate_with_ir_cache(
            &bad_ast, &path, false, &build, &trace,
        );
        assert!(
            result.is_err(),
            "a broken source must fail, not be served the clean run's cache entry"
        );
    }
}
