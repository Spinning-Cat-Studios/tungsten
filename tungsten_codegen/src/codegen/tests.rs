use super::*;
use inkwell::context::Context;
use std::collections::HashMap;

#[test]
fn test_codegen_new() {
    let context = Context::create();
    let codegen = CodeGen::new(&context, "test");
    assert!(codegen.module.get_function("printf").is_some());
    // ADR 14.9.26b: the runtime symbol and its prologue init.
    assert!(codegen.module.get_function("__tungsten_alloc").is_some());
    assert!(codegen
        .module
        .get_function("__tungsten_arena_init")
        .is_some());
    // ADR 18.9.26c: mode off calls `malloc` directly, gated on the flag.
    assert!(codegen.module.get_function("malloc").is_some());
    let mode = codegen
        .module
        .get_global("__tungsten_arena_mode")
        .expect("mode flag declared");
    assert!(
        mode.get_initializer().is_none(),
        "external, not defined here"
    );
}

#[test]
fn test_fresh_name() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    let name1 = codegen.fresh_name("var");
    let name2 = codegen.fresh_name("var");
    assert_ne!(name1, name2);
    assert!(name1.starts_with("var_"));
    assert!(name2.starts_with("var_"));
}

#[test]
fn test_fresh_lambda_name() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    let name1 = codegen.fresh_lambda_name();
    let name2 = codegen.fresh_lambda_name();
    assert_ne!(name1, name2);
    assert!(name1.starts_with("__lambda_"));
}

/// ADR 12.7.26c P6/D7: inside a synthesized def, lambdas name deterministically
/// as `<def-symbol>$l<index>` regardless of `named_lambdas`, and the index
/// resets per def. Ordinary code (no synthesized binding) keeps `__lambda_N`.
#[test]
fn test_synthesized_lambda_naming_is_deterministic_and_scoped() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");

    // Ordinary code: __lambda_N even without named_lambdas.
    assert!(codegen.fresh_lambda_name().starts_with("__lambda_"));

    // Enter synthesized mode for `compare_List_Nat_spine`.
    codegen.naming.synthesized_def_binding = Some("compare_List_Nat_spine".to_string());
    codegen.naming.synthesized_lambda_index = 0;
    assert_eq!(codegen.fresh_lambda_name(), "compare_List_Nat_spine$l0");
    assert_eq!(codegen.fresh_lambda_name(), "compare_List_Nat_spine$l1");

    // Leaving synthesized mode restores ordinary naming.
    codegen.naming.synthesized_def_binding = None;
    assert!(codegen.fresh_lambda_name().starts_with("__lambda_"));
}

/// The synthesized scheme must win even when `--named-lambdas` is on.
#[test]
fn test_synthesized_naming_overrides_named_lambdas() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen.set_named_lambdas(true);
    codegen.naming.current_binding_name = Some("some_binding".to_string());
    codegen.naming.synthesized_def_binding = Some("compare_Nat".to_string());
    codegen.naming.synthesized_lambda_index = 0;
    assert_eq!(codegen.fresh_lambda_name(), "compare_Nat$l0");
}

#[test]
fn test_register_extern_name_map() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    let mut map = HashMap::new();
    map.insert("original".to_string(), "remapped".to_string());
    codegen.register_extern_name_map(map);
    assert_eq!(
        codegen.defs.extern_name_map.get("original"),
        Some(&"remapped".to_string())
    );
}

#[test]
fn test_type_size_bytes() {
    let context = Context::create();
    let codegen = CodeGen::new(&context, "test");

    let i8_ty = context.i8_type().into();
    assert_eq!(codegen.type_size_bytes(i8_ty), 1);

    let i64_ty = context.i64_type().into();
    assert_eq!(codegen.type_size_bytes(i64_ty), 8);

    let ptr_ty = context.ptr_type(AddressSpace::default()).into();
    assert_eq!(codegen.type_size_bytes(ptr_ty), 8);
}

#[test]
fn test_type_size_bytes_struct_alignment() {
    // Test that struct sizes account for LLVM alignment padding
    let context = Context::create();
    let codegen = CodeGen::new(&context, "test");

    let i8_ty = context.i8_type();
    let i64_ty = context.i64_type();
    let ptr_ty = context.ptr_type(AddressSpace::default());

    // Simple struct: { i64, i64 } should be 16 bytes
    let simple_struct = context.struct_type(&[i64_ty.into(), i64_ty.into()], false);
    let simple_size = codegen.type_size_bytes(simple_struct.into());
    assert!(
        simple_size >= 16,
        "{{ i64, i64 }} should be >= 16 bytes, got {}",
        simple_size
    );

    // Struct with padding: { i8, i64 } - i8 needs padding before i64
    let padded_struct = context.struct_type(&[i8_ty.into(), i64_ty.into()], false);
    let padded_size = codegen.type_size_bytes(padded_struct.into());
    // LLVM aligns i64 to 8 bytes, so i8 + 7 padding + i64 = 16
    assert!(
        padded_size >= 16,
        "{{ i8, i64 }} should be >= 16 bytes (alignment), got {}",
        padded_size
    );

    // Nested struct simulating Token × ptr (like List<Token> Cons payload)
    // TokenKind ≈ { i32, [64 x i8] }
    let payload_array = context.i8_type().array_type(64);
    let token_kind_ty =
        context.struct_type(&[context.i32_type().into(), payload_array.into()], false);
    // Span ≈ { i64, i64, { ptr, i64 } }
    let string_ty = context.struct_type(&[ptr_ty.into(), i64_ty.into()], false);
    let span_ty = context.struct_type(&[i64_ty.into(), i64_ty.into(), string_ty.into()], false);
    // Token = { TokenKind, Span }
    let token_ty = context.struct_type(&[token_kind_ty.into(), span_ty.into()], false);
    // Cons payload = { Token, ptr }
    let cons_payload = context.struct_type(&[token_ty.into(), ptr_ty.into()], false);

    let cons_size = codegen.type_size_bytes(cons_payload.into());
    // Must be large enough to hold the actual LLVM layout
    assert!(
        cons_size >= 100,
        "Cons<Token> payload should be >= 100 bytes, got {}",
        cons_size
    );
}

#[test]
fn test_module_prefix_lambda_names() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen.set_module_prefix("driver".to_string());
    let name1 = codegen.fresh_lambda_name();
    let name2 = codegen.fresh_lambda_name();
    assert!(name1.contains("driver"), "expected 'driver' in '{}'", name1);
    assert!(name2.contains("driver"), "expected 'driver' in '{}'", name2);
    assert_ne!(name1, name2);
    assert!(name1.starts_with("__driver_lambda_"));
}

#[test]
fn test_module_prefix_fresh_name() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen.set_module_prefix("lexer".to_string());
    let name = codegen.fresh_name("fix");
    assert!(name.contains("lexer"), "expected 'lexer' in '{}'", name);
}

#[test]
fn test_no_module_prefix_default() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    let lambda = codegen.fresh_lambda_name();
    assert!(
        lambda.starts_with("__lambda_"),
        "expected __lambda_ prefix, got '{}'",
        lambda
    );
    let fresh = codegen.fresh_name("fix");
    assert!(
        fresh.starts_with("fix_"),
        "expected fix_ prefix, got '{}'",
        fresh
    );
}

#[test]
fn test_module_prefix_mono_name_format() {
    // Monomorphized names use format!("{}__mono_{}_{}", name, prefix, counter)
    // when module_prefix is set. Verify the naming pattern directly.
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen.set_module_prefix("parser".to_string());

    // Simulate the mono naming logic from polymorphism.rs
    let name = "list_reverse";
    let mono_name = if let Some(ref prefix) = codegen.naming.module_prefix {
        codegen.naming.counter += 1;
        format!("{}__mono_{}_{}", name, prefix, codegen.naming.counter)
    } else {
        codegen.naming.counter += 1;
        format!("{}__mono_{}", name, codegen.naming.counter)
    };
    assert_eq!(mono_name, "list_reverse__mono_parser_1");

    // Without prefix
    let context2 = Context::create();
    let mut codegen2 = CodeGen::new(&context2, "test2");
    let mono_name2 = if let Some(ref prefix) = codegen2.naming.module_prefix {
        codegen2.naming.counter += 1;
        format!("{}__mono_{}_{}", name, prefix, codegen2.naming.counter)
    } else {
        codegen2.naming.counter += 1;
        format!("{}__mono_{}", name, codegen2.naming.counter)
    };
    assert_eq!(mono_name2, "list_reverse__mono_1");
}
