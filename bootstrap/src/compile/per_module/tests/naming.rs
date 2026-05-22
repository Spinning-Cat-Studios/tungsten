use crate::compile::per_module::*;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

#[test]
fn test_codegen_unit_name_root() {
    let source = Path::new("/project/src/main.tg");
    let root = Path::new("/project/src");
    assert_eq!(codegen_unit_name(source, root, "entry"), "main__entry");
}

#[test]
fn test_codegen_unit_name_single() {
    let source = Path::new("/project/src/lexer.tg");
    let root = Path::new("/project/src");
    assert_eq!(codegen_unit_name(source, root, "scan"), "lexer__scan");
}

#[test]
fn test_codegen_unit_name_nested() {
    let source = Path::new("/project/src/parser/ast.tg");
    let root = Path::new("/project/src");
    assert_eq!(codegen_unit_name(source, root, "node"), "parser__ast__node");
}

#[test]
fn test_codegen_unit_name_deeply_nested() {
    let source = Path::new("/project/src/elab/env/defs.tg");
    let root = Path::new("/project/src");
    assert_eq!(
        codegen_unit_name(source, root, "lookup_type"),
        "elab__env__defs__lookup_type"
    );
}

#[test]
fn test_codegen_unit_name_mod_file() {
    let source = Path::new("/project/src/driver/mod.tg");
    let root = Path::new("/project/src");
    assert_eq!(
        codegen_unit_name(source, root, "elaborate"),
        "driver__mod__elaborate"
    );
}

#[test]
fn test_codegen_unit_name_main_def() {
    let source = Path::new("/project/src/main.tg");
    let root = Path::new("/project/src");
    assert_eq!(
        codegen_unit_name(source, root, "main"),
        "main__tungsten_main"
    );
}

#[test]
fn test_file_unit_base() {
    let source = Path::new("/project/src/elab/env/defs.tg");
    let root = Path::new("/project/src");
    assert_eq!(file_unit_base(source, root), "elab__env__defs");
}

#[test]
fn test_scoped_llvm_name_main() {
    let collisions = HashSet::new();
    assert_eq!(
        scoped_llvm_name("main", "driver", &collisions),
        "tungsten_main"
    );
}

#[test]
fn test_scoped_llvm_name_no_collision() {
    let collisions = HashSet::new();
    assert_eq!(scoped_llvm_name("helper", "alpha", &collisions), "helper");
    assert_eq!(scoped_llvm_name("compute", "beta", &collisions), "compute");
}

#[test]
fn test_scoped_llvm_name_with_collision() {
    let mut collisions = HashSet::new();
    collisions.insert("helper".to_string());

    assert_eq!(
        scoped_llvm_name("helper", "alpha", &collisions),
        "alpha__helper"
    );
    assert_eq!(
        scoped_llvm_name("helper", "beta", &collisions),
        "beta__helper"
    );
    assert_eq!(scoped_llvm_name("compute", "alpha", &collisions), "compute");
}

/// Helper to build a single-def ModuleCodegenUnit for tests (ADR 9.5.26b).
fn make_unit(source_file: &str, path: &[&str], def_name: &str) -> ModuleCodegenUnit {
    use tungsten_bootstrap::elaborate::CoreDef;
    use tungsten_bootstrap::Span;
    use tungsten_core::terms::SpannedTerm;

    ModuleCodegenUnit {
        module_path: path.iter().map(|s| s.to_string()).collect(),
        source_file: PathBuf::from(source_file),
        defs: vec![CoreDef {
            name: def_name.to_string(),
            ty: Type::Nat,
            term: SpannedTerm {
                term: Term::Zero,
                span: None,
            },
            span: Span::new(0, 0),
        }],
    }
}

#[test]
fn test_find_colliding_names_empty() {
    let units = vec![
        make_unit("/project/src/alpha.tg", &["alpha"], "compute"),
        make_unit("/project/src/alpha.tg", &["alpha"], "helper_a"),
        make_unit("/project/src/beta.tg", &["beta"], "greet"),
        make_unit("/project/src/beta.tg", &["beta"], "helper_b"),
    ];
    let collisions = find_colliding_names(&units);
    assert!(collisions.is_empty());
}

#[test]
fn test_find_colliding_names_with_duplicates() {
    let units = vec![
        make_unit("/project/src/alpha.tg", &["alpha"], "compute"),
        make_unit("/project/src/alpha.tg", &["alpha"], "helper"),
        make_unit("/project/src/beta.tg", &["beta"], "greet"),
        make_unit("/project/src/beta.tg", &["beta"], "helper"),
    ];
    let collisions = find_colliding_names(&units);
    assert_eq!(collisions.len(), 1);
    assert!(collisions.contains("helper"));
}

#[test]
fn test_find_colliding_names_main_becomes_tungsten_main() {
    let units = vec![
        make_unit("/project/src/alpha.tg", &["alpha"], "main"),
        make_unit("/project/src/beta.tg", &["beta"], "main"),
    ];
    let collisions = find_colliding_names(&units);
    assert_eq!(collisions.len(), 1);
    assert!(collisions.contains("tungsten_main"));
}

#[test]
fn test_resolve_emit_llvm_dir_with_file_output() {
    let file = PathBuf::from("/project/src/main.tg");
    let output = std::path::Path::new("/build/output.ll");
    assert_eq!(
        resolve_emit_llvm_dir(&file, Some(output)),
        PathBuf::from("/build")
    );
}

#[test]
fn test_resolve_emit_llvm_dir_with_dir_output() {
    let file = PathBuf::from("/project/src/main.tg");
    let output = std::path::Path::new("/build/out");
    assert_eq!(
        resolve_emit_llvm_dir(&file, Some(output)),
        PathBuf::from("/build/out")
    );
}

#[test]
fn test_resolve_emit_llvm_dir_no_output() {
    let file = PathBuf::from("/project/src/main.tg");
    assert_eq!(
        resolve_emit_llvm_dir(&file, None),
        PathBuf::from("/project/src/target/ll")
    );
}

#[test]
fn test_per_function_unit_naming() {
    let root = Path::new("/project/src");
    let source = Path::new("/project/src/elab/env/defs.tg");
    assert_eq!(
        codegen_unit_name(source, root, "lookup_type"),
        "elab__env__defs__lookup_type"
    );
    assert_eq!(
        codegen_unit_name(source, root, "insert_type"),
        "elab__env__defs__insert_type"
    );
    assert_eq!(file_unit_base(source, root), "elab__env__defs");
}

#[test]
fn test_per_function_units_from_same_file() {
    let units = vec![
        make_unit("/project/src/main.tg", &[], "main_fn"),
        make_unit("/project/src/parser.tg", &["parser"], "parse"),
        make_unit("/project/src/parser/ast.tg", &["parser", "ast"], "node"),
    ];
    assert_eq!(units.len(), 3);
    let root = Path::new("/project/src");
    let names: Vec<String> = units
        .iter()
        .map(|u| codegen_unit_name(&u.source_file, root, &u.defs[0].name))
        .collect();
    assert_eq!(
        names,
        vec!["main__main_fn", "parser__parse", "parser__ast__node"]
    );
}

#[test]
fn test_source_file_outside_root() {
    let root = Path::new("/project/src");
    let outside = Path::new("/other/lib/helper.tg");
    assert!(outside.strip_prefix(root).is_err());
}

#[test]
fn test_codegen_unit_name_special_characters() {
    let root = Path::new("/project/src");
    let source = Path::new("/project/src/ops.tg");

    // Names with underscores pass through unchanged
    assert_eq!(codegen_unit_name(source, root, "add_nat"), "ops__add_nat");

    // The `__` separator means a def named "a__b" looks like a deeper path,
    // but this is the expected behavior — no disambiguation is applied yet (ADR 9.5.26b §2.4.1)
    assert_eq!(codegen_unit_name(source, root, "a__b"), "ops__a__b");

    // Empty def name (edge case)
    assert_eq!(codegen_unit_name(source, root, ""), "ops__");

    // Name that starts with a digit
    assert_eq!(
        codegen_unit_name(source, root, "42_answer"),
        "ops__42_answer"
    );

    // "main" gets renamed to "tungsten_main"
    assert_eq!(
        codegen_unit_name(source, root, "main"),
        "ops__tungsten_main"
    );
}
