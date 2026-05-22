use super::{discover_tests, paint, scope_defs_to_module, CoreDef, ModuleScopeResult, Style};
use crate::cli::{Cli, ColorMode, Commands};
use clap::Parser;
use std::path::{Path, PathBuf};
use tungsten_bootstrap::span::Span;
use tungsten_core::terms::SpannedTerm;
use tungsten_core::Type;

fn make_def(name: &str, ty: Type) -> CoreDef {
    CoreDef {
        name: name.to_string(),
        ty,
        term: SpannedTerm::generated(tungsten_core::Term::Unit),
        span: Span::new(0, 0),
    }
}

#[test]
fn discover_finds_test_functions() {
    let defs = vec![
        make_def("test_add", Type::Unit),
        make_def("helper", Type::Unit),
        make_def("test_sub", Type::Unit),
    ];
    let (tests, errors) = discover_tests(&defs, None);
    assert_eq!(tests.len(), 2);
    assert_eq!(tests[0].name, "test_add");
    assert_eq!(tests[1].name, "test_sub");
    assert!(errors.is_empty());
}

#[test]
fn discover_rejects_arrow_type() {
    let defs = vec![make_def(
        "test_with_args",
        Type::Arrow(Box::new(Type::Nat), Box::new(Type::Unit)),
    )];
    let (tests, errors) = discover_tests(&defs, None);
    assert!(tests.is_empty());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].reason.contains("no parameters"));
}

#[test]
fn discover_rejects_non_unit_return() {
    let defs = vec![make_def("test_returns_nat", Type::Nat)];
    let (tests, errors) = discover_tests(&defs, None);
    assert!(tests.is_empty());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].reason.contains("return Unit"));
}

#[test]
fn discover_filter_by_substring() {
    let defs = vec![
        make_def("test_add", Type::Unit),
        make_def("test_sub", Type::Unit),
        make_def("test_addition", Type::Unit),
    ];
    let (tests, _) = discover_tests(&defs, Some("add"));
    assert_eq!(tests.len(), 2);
    assert_eq!(tests[0].name, "test_add");
    assert_eq!(tests[1].name, "test_addition");
}

#[test]
fn discover_skips_non_test_functions() {
    let defs = vec![
        make_def("main", Type::Unit),
        make_def("helper", Type::Nat),
        make_def("testing_util", Type::Unit),
    ];
    let (tests, errors) = discover_tests(&defs, None);
    assert!(tests.is_empty());
    assert!(errors.is_empty());
}

#[test]
fn discover_empty_defs() {
    let (tests, errors) = discover_tests(&[], None);
    assert!(tests.is_empty());
    assert!(errors.is_empty());
}

// --- Module scoping tests (ADR 12.5.26b) ---

fn make_module_defs() -> Vec<(Vec<String>, PathBuf, Vec<CoreDef>)> {
    vec![
        (
            vec!["elab".into(), "env".into()],
            PathBuf::from("/project/src/compiler/elab/env/mod.tg"),
            vec![
                make_def("test_lookup", Type::Unit),
                make_def("helper", Type::Unit),
            ],
        ),
        (
            vec!["elab".into(), "types".into()],
            PathBuf::from("/project/src/compiler/elab/types/mod.tg"),
            vec![
                make_def("test_encode", Type::Unit),
                make_def("test_decode", Type::Unit),
            ],
        ),
        (
            vec!["driver".into()],
            PathBuf::from("/project/src/compiler/driver/mod.tg"),
            vec![make_def("test_driver_init", Type::Unit)],
        ),
    ]
}

#[test]
fn scope_exact_path_match() {
    let module_defs = make_module_defs();
    let result = scope_defs_to_module(
        &module_defs,
        "src/compiler/elab/env/mod.tg",
        Path::new("/project"),
    );
    match result {
        ModuleScopeResult::Matched(defs) => {
            assert_eq!(defs.len(), 2);
            assert_eq!(defs[0].name, "test_lookup");
        }
        other => panic!("expected Matched, got {:?}", other),
    }
}

#[test]
fn scope_dotslash_normalizes() {
    let module_defs = make_module_defs();
    // "./src/compiler/elab/env/mod.tg" should match the same as without "./"
    let result = scope_defs_to_module(
        &module_defs,
        "./src/compiler/elab/env/mod.tg",
        Path::new("/project"),
    );
    match result {
        ModuleScopeResult::Matched(defs) => {
            assert_eq!(defs.len(), 2);
        }
        other => panic!("expected Matched, got {:?}", other),
    }
}

#[test]
fn scope_no_match_returns_nomatch() {
    let module_defs = make_module_defs();
    let result = scope_defs_to_module(
        &module_defs,
        "src/compiler/nonexistent/mod.tg",
        Path::new("/project"),
    );
    assert!(matches!(result, ModuleScopeResult::NoMatch));
}

#[test]
fn scope_suffix_match() {
    let module_defs = make_module_defs();
    // "elab/env/mod.tg" is a suffix of "/project/src/compiler/elab/env/mod.tg"
    let result = scope_defs_to_module(&module_defs, "elab/env/mod.tg", Path::new("/project"));
    match result {
        ModuleScopeResult::Matched(defs) => {
            assert_eq!(defs.len(), 2);
            assert_eq!(defs[0].name, "test_lookup");
        }
        other => panic!("expected Matched, got {:?}", other),
    }
}

#[test]
fn scope_ambiguous_suffix() {
    // Create two modules that both end with "mod.tg" under different elab/ paths
    let module_defs = vec![
        (
            vec!["a".into()],
            PathBuf::from("/project/src/a/mod.tg"),
            vec![make_def("test_a", Type::Unit)],
        ),
        (
            vec!["b".into()],
            PathBuf::from("/project/src/b/mod.tg"),
            vec![make_def("test_b", Type::Unit)],
        ),
    ];
    let result = scope_defs_to_module(&module_defs, "mod.tg", Path::new("/project"));
    match result {
        ModuleScopeResult::Ambiguous(paths) => {
            assert_eq!(paths.len(), 2);
        }
        other => panic!("expected Ambiguous, got {:?}", other),
    }
}

#[test]
fn scope_composes_with_filter() {
    // Verify that scoping + filtering work together:
    // scope to elab/types → 2 test defs, then filter by "encode" → 1 test
    let module_defs = make_module_defs();
    let scoped =
        match scope_defs_to_module(&module_defs, "elab/types/mod.tg", Path::new("/project")) {
            ModuleScopeResult::Matched(defs) => defs,
            other => panic!("expected Matched, got {:?}", other),
        };
    assert_eq!(scoped.len(), 2);

    let (tests, _) = discover_tests(&scoped, Some("encode"));
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].name, "test_encode");
}

#[test]
fn scope_absolute_path_match() {
    let module_defs = make_module_defs();
    // Absolute path should match directly without joining to project_root
    let result = scope_defs_to_module(
        &module_defs,
        "/project/src/compiler/elab/env/mod.tg",
        Path::new("/project"),
    );
    match result {
        ModuleScopeResult::Matched(defs) => {
            assert_eq!(defs.len(), 2);
            assert_eq!(defs[0].name, "test_lookup");
        }
        other => panic!("expected Matched, got {:?}", other),
    }
}

#[test]
fn scope_empty_module_defs() {
    let module_defs: Vec<(Vec<String>, PathBuf, Vec<CoreDef>)> = vec![];
    let result = scope_defs_to_module(&module_defs, "anything.tg", Path::new("/project"));
    assert!(matches!(result, ModuleScopeResult::NoMatch));
}

// --- paint() tests (ADR 12.5.26d) ---

#[test]
fn paint_no_color_returns_unchanged() {
    assert_eq!(paint("ok", Style::Green, false), "ok");
    assert_eq!(paint("FAILED", Style::Red, false), "FAILED");
    assert_eq!(paint("skipped", Style::Yellow, false), "skipped");
    assert_eq!(paint("result:", Style::Bold, false), "result:");
    assert_eq!(paint("ok", Style::BoldGreen, false), "ok");
    assert_eq!(paint("FAILED", Style::BoldRed, false), "FAILED");
}

#[test]
fn paint_with_color_wraps_ansi() {
    assert_eq!(paint("ok", Style::Green, true), "\x1b[32mok\x1b[0m");
    assert_eq!(paint("FAILED", Style::Red, true), "\x1b[31mFAILED\x1b[0m");
    assert_eq!(
        paint("skipped", Style::Yellow, true),
        "\x1b[33mskipped\x1b[0m"
    );
    assert_eq!(paint("result:", Style::Bold, true), "\x1b[1mresult:\x1b[0m");
    assert_eq!(paint("ok", Style::BoldGreen, true), "\x1b[1;32mok\x1b[0m");
    assert_eq!(
        paint("FAILED", Style::BoldRed, true),
        "\x1b[1;31mFAILED\x1b[0m"
    );
}

#[test]
fn paint_no_color_no_ansi_escapes() {
    for style in [
        Style::Green,
        Style::Red,
        Style::Yellow,
        Style::Bold,
        Style::BoldGreen,
        Style::BoldRed,
    ] {
        let result = paint("test", style, false);
        assert!(
            !result.contains('\x1b'),
            "use_color=false should never contain ANSI escapes"
        );
    }
}

#[test]
fn color_auto_is_default() {
    let cli = Cli::try_parse_from(["tungsten", "test", "file.tg"]).unwrap();
    match cli.command {
        Some(Commands::Test { color, .. }) => {
            assert!(
                matches!(color, ColorMode::Auto),
                "expected Auto, got {:?}",
                color
            );
        }
        other => panic!("expected Test command, got {:?}", other.map(|_| "other")),
    }
}
