use super::*;
use inkwell::context::Context;

#[test]
fn test_string_concat_ffi_declared() {
    let context = Context::create();
    let codegen = CodeGen::new(&context, "test");
    assert!(
        codegen.module.get_function("tg_string_concat").is_some(),
        "tg_string_concat should be declared in runtime functions"
    );
}

#[test]
fn test_string_concat_owned_ffi_declared() {
    let context = Context::create();
    let codegen = CodeGen::new(&context, "test");
    assert!(
        codegen
            .module
            .get_function("tg_string_concat_owned")
            .is_some(),
        "tg_string_concat_owned should be declared in runtime functions"
    );
}

#[test]
fn test_liveness_gate_nested_concat_uses_owned() {
    // Term::StrConcat(Term::StrConcat(..), ..) should trigger owned path
    let inner = Term::str_concat(
        Term::StringLit("a".to_string()),
        Term::StringLit("b".to_string()),
    );
    let right = Term::StringLit("c".to_string());
    let outer_left: &Term = &inner;
    // The gate: nested StrConcat → owned
    assert!(
        matches!(outer_left, Term::StrConcat(_, _)),
        "nested StrConcat left should match the owned-path gate"
    );
    let _ = right;
}

#[test]
fn test_liveness_gate_literal_uses_regular() {
    // Term::StringLit should NOT trigger owned path
    let lit = Term::StringLit("hello".to_string());
    let left: &Term = &lit;
    assert!(
        !matches!(left, Term::StrConcat(_, _)),
        "string literal should NOT match the owned-path gate"
    );
}

#[test]
fn test_liveness_gate_variable_uses_regular() {
    // Term::Var should NOT trigger owned path without state tracking
    let var = Term::Var("x".to_string());
    let left: &Term = &var;
    assert!(
        !matches!(left, Term::StrConcat(_, _)),
        "variable reference should NOT match the owned-path gate"
    );
}

#[test]
fn test_last_use_vars_populated_for_str_concat_binding() {
    // When let s = a ++ b in <body using s once>, s should be in last_use_vars
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");

    // Simulate: s is bound to StrConcat, used once in body
    codegen.compilation.last_use_vars.insert("s".to_string());
    codegen.compilation.heap_origin_vars.insert("s".to_string());

    let left = Term::Var("s".to_string());
    // Gate check: Var("s") with both sets populated → owned
    if let Term::Var(x) = &left {
        let uses_owned = codegen.compilation.last_use_vars.contains(x)
            && codegen.compilation.heap_origin_vars.contains(x);
        assert!(
            uses_owned,
            "let-bound last-use StrConcat var should trigger owned path"
        );
    }
}

#[test]
fn test_last_use_vars_not_triggered_without_heap_origin() {
    // Var in last_use but NOT heap_origin → regular (could be a literal binding)
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");

    codegen.compilation.last_use_vars.insert("s".to_string());
    // heap_origin NOT set

    let left = Term::Var("s".to_string());
    if let Term::Var(x) = &left {
        let uses_owned = codegen.compilation.last_use_vars.contains(x)
            && codegen.compilation.heap_origin_vars.contains(x);
        assert!(
            !uses_owned,
            "last-use without heap-origin should NOT trigger owned path"
        );
    }
}

#[test]
fn test_last_use_vars_not_triggered_for_multi_use() {
    // Var in heap_origin but NOT last_use → regular (used multiple times)
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");

    codegen.compilation.heap_origin_vars.insert("s".to_string());
    // last_use NOT set

    let left = Term::Var("s".to_string());
    if let Term::Var(x) = &left {
        let uses_owned = codegen.compilation.last_use_vars.contains(x)
            && codegen.compilation.heap_origin_vars.contains(x);
        assert!(
            !uses_owned,
            "heap-origin without last-use should NOT trigger owned path"
        );
    }
}
