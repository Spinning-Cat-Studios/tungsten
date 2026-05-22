use crate::compile::per_module::*;
use compilation::collect_referenced_globals;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

/// Helper to build a single-def ModuleCodegenUnit for tests.
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

/// Helper to build a ModuleCodegenUnit with a custom term (for referenced-globals tests).
fn make_unit_with_term(
    source_file: &str,
    path: &[&str],
    def_name: &str,
    term: Term,
) -> ModuleCodegenUnit {
    use tungsten_bootstrap::elaborate::CoreDef;
    use tungsten_bootstrap::Span;
    use tungsten_core::terms::SpannedTerm;
    use tungsten_core::Type;

    ModuleCodegenUnit {
        module_path: path.iter().map(|s| s.to_string()).collect(),
        source_file: PathBuf::from(source_file),
        defs: vec![CoreDef {
            name: def_name.to_string(),
            ty: Type::Nat,
            term: SpannedTerm { term, span: None },
            span: Span::new(0, 0),
        }],
    }
}

/// Helper to build a ModuleCodegenUnit with a custom type + term.
fn make_unit_typed(
    source_file: &str,
    path: &[&str],
    def_name: &str,
    ty: Type,
    term: Term,
) -> ModuleCodegenUnit {
    use tungsten_bootstrap::elaborate::CoreDef;
    use tungsten_bootstrap::Span;
    use tungsten_core::terms::SpannedTerm;

    ModuleCodegenUnit {
        module_path: path.iter().map(|s| s.to_string()).collect(),
        source_file: PathBuf::from(source_file),
        defs: vec![CoreDef {
            name: def_name.to_string(),
            ty,
            term: SpannedTerm { term, span: None },
            span: Span::new(0, 0),
        }],
    }
}

#[test]
fn test_collect_referenced_globals_empty() {
    let unit = make_unit("/project/src/a.tg", &["a"], "f");
    let refs = collect_referenced_globals(&unit);
    assert!(refs.is_empty());
}

#[test]
fn test_collect_referenced_globals_direct() {
    let term = Term::App(
        Box::new(Term::Global("foo".to_string())),
        Box::new(Term::Global("bar".to_string())),
    );
    let unit = make_unit_with_term("/project/src/a.tg", &["a"], "f", term);
    let refs = collect_referenced_globals(&unit);
    assert_eq!(refs.len(), 2);
    assert!(refs.contains("foo"));
    assert!(refs.contains("bar"));
}

#[test]
fn test_collect_referenced_globals_nested() {
    let term = Term::Let(
        "x".to_string(),
        tungsten_core::types::Type::Nat,
        Box::new(Term::Global("inner_fn".to_string())),
        Box::new(Term::Lambda(
            "y".to_string(),
            tungsten_core::types::Type::Nat,
            Box::new(Term::Global("deep_fn".to_string())),
        )),
    );
    let unit = make_unit_with_term("/project/src/a.tg", &["a"], "f", term);
    let refs = collect_referenced_globals(&unit);
    assert_eq!(refs.len(), 2);
    assert!(refs.contains("inner_fn"));
    assert!(refs.contains("deep_fn"));
}

#[test]
fn test_collect_referenced_globals_deduplicates() {
    let term = Term::App(
        Box::new(Term::Global("foo".to_string())),
        Box::new(Term::Global("foo".to_string())),
    );
    let unit = make_unit_with_term("/project/src/a.tg", &["a"], "f", term);
    let refs = collect_referenced_globals(&unit);
    assert_eq!(refs.len(), 1);
    assert!(refs.contains("foo"));
}

#[test]
fn test_collect_referenced_globals_captures_cross_module_refs() {
    let term = Term::Let(
        "result".to_string(),
        tungsten_core::types::Type::Nat,
        Box::new(Term::App(
            Box::new(Term::Global("foreign_helper".to_string())),
            Box::new(Term::Zero),
        )),
        Box::new(Term::Global("another_module_fn".to_string())),
    );
    let unit = make_unit_with_term("/project/src/caller.tg", &["caller"], "invoke", term);
    let refs = collect_referenced_globals(&unit);
    assert!(refs.contains("foreign_helper"));
    assert!(refs.contains("another_module_fn"));
    assert_eq!(refs.len(), 2);
}

#[test]
fn test_collect_referenced_globals_includes_self_refs() {
    let term = Term::App(
        Box::new(Term::Global("my_own_fn".to_string())),
        Box::new(Term::Global("foreign_fn".to_string())),
    );
    let unit = make_unit_with_term("/project/src/a.tg", &["a"], "my_own_fn", term);
    let refs = collect_referenced_globals(&unit);
    assert!(refs.contains("my_own_fn"));
    assert!(refs.contains("foreign_fn"));
}

#[test]
fn test_register_term_defs_only_forall() {
    use compilation::register_term_defs;

    let poly_unit = make_unit_typed(
        "/project/src/a.tg",
        &["a"],
        "identity",
        Type::Forall(
            "T".to_string(),
            Box::new(Type::Arrow(
                Box::new(Type::TyVar("T".to_string())),
                Box::new(Type::TyVar("T".to_string())),
            )),
        ),
        Term::Lambda(
            "x".to_string(),
            Type::TyVar("T".to_string()),
            Box::new(Term::Var("x".to_string())),
        ),
    );
    let mono_unit = make_unit_typed(
        "/project/src/b.tg",
        &["b"],
        "add_one",
        Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)),
        Term::Lambda(
            "n".to_string(),
            Type::Nat,
            Box::new(Term::Succ(Box::new(Term::Var("n".to_string())))),
        ),
    );

    let units = vec![poly_unit, mono_unit];
    let collisions = find_colliding_names(&units);
    let root = Path::new("/project/src");

    let llvm_context = tungsten_codegen::inkwell::context::Context::create();
    let mut codegen = tungsten_codegen::CodeGen::new(&llvm_context, "test");

    register_term_defs(&mut codegen, &units, &collisions, root);

    assert!(
        codegen.has_term_def("identity"),
        "Forall-typed 'identity' should be registered"
    );
    assert!(
        !codegen.has_term_def("add_one"),
        "Monomorphic 'add_one' should NOT be registered"
    );
}

#[test]
fn test_build_poly_term_registry_matches_register_term_defs() {
    use compilation::{build_poly_term_registry, register_term_defs};

    let poly_unit = make_unit_typed(
        "/project/src/a.tg",
        &["a"],
        "identity",
        Type::Forall(
            "T".to_string(),
            Box::new(Type::Arrow(
                Box::new(Type::TyVar("T".to_string())),
                Box::new(Type::TyVar("T".to_string())),
            )),
        ),
        Term::Lambda(
            "x".to_string(),
            Type::TyVar("T".to_string()),
            Box::new(Term::Var("x".to_string())),
        ),
    );
    let mono_unit = make_unit_typed(
        "/project/src/b.tg",
        &["b"],
        "add_one",
        Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)),
        Term::Lambda(
            "n".to_string(),
            Type::Nat,
            Box::new(Term::Succ(Box::new(Term::Var("n".to_string())))),
        ),
    );

    let units = vec![poly_unit, mono_unit];
    let collisions = find_colliding_names(&units);
    let root = Path::new("/project/src");

    let registry = build_poly_term_registry(&units, &collisions, root);

    let llvm_context = tungsten_codegen::inkwell::context::Context::create();
    let mut codegen = tungsten_codegen::CodeGen::new(&llvm_context, "test");
    register_term_defs(&mut codegen, &units, &collisions, root);

    assert!(registry.contains_key("identity"));
    assert!(!registry.contains_key("add_one"));

    let mut codegen2 = tungsten_codegen::CodeGen::new(&llvm_context, "test2");
    codegen2.register_term_defs_bulk(&registry);
    assert!(codegen2.has_term_def("identity"));
    assert!(!codegen2.has_term_def("add_one"));
}

#[test]
fn test_register_term_defs_bulk_clones_all_entries() {
    use compilation::build_poly_term_registry;

    let poly_a = make_unit_typed(
        "/project/src/a.tg",
        &["a"],
        "id",
        Type::Forall(
            "T".to_string(),
            Box::new(Type::Arrow(
                Box::new(Type::TyVar("T".to_string())),
                Box::new(Type::TyVar("T".to_string())),
            )),
        ),
        Term::Lambda(
            "x".to_string(),
            Type::TyVar("T".to_string()),
            Box::new(Term::Var("x".to_string())),
        ),
    );
    let poly_b = make_unit_typed(
        "/project/src/b.tg",
        &["b"],
        "const_fn",
        Type::Forall(
            "A".to_string(),
            Box::new(Type::Forall(
                "B".to_string(),
                Box::new(Type::Arrow(
                    Box::new(Type::TyVar("A".to_string())),
                    Box::new(Type::Arrow(
                        Box::new(Type::TyVar("B".to_string())),
                        Box::new(Type::TyVar("A".to_string())),
                    )),
                )),
            )),
        ),
        Term::Lambda(
            "a".to_string(),
            Type::TyVar("A".to_string()),
            Box::new(Term::Lambda(
                "b".to_string(),
                Type::TyVar("B".to_string()),
                Box::new(Term::Var("a".to_string())),
            )),
        ),
    );
    let mono_unit = make_unit_typed(
        "/project/src/c.tg",
        &["c"],
        "inc",
        Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)),
        Term::Lambda(
            "n".to_string(),
            Type::Nat,
            Box::new(Term::Succ(Box::new(Term::Var("n".to_string())))),
        ),
    );

    let units = vec![poly_a, poly_b, mono_unit];
    let collisions = find_colliding_names(&units);
    let root = Path::new("/project/src");
    let registry = build_poly_term_registry(&units, &collisions, root);

    let llvm_context = tungsten_codegen::inkwell::context::Context::create();
    let mut codegen = tungsten_codegen::CodeGen::new(&llvm_context, "test");
    codegen.register_term_defs_bulk(&registry);

    for key in registry.keys() {
        assert!(
            codegen.has_term_def(key),
            "bulk-registered key '{}' should be findable",
            key
        );
    }
    assert!(
        !codegen.has_term_def("inc"),
        "monomorphic 'inc' should not be in registry"
    );
}
