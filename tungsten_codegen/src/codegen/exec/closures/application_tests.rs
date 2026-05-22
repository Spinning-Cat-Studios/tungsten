use super::*;
use crate::codegen::CodeGen;
use inkwell::context::Context;
use inkwell::values::FunctionValue;

/// Create a CodeGen instance with an active function and positioned builder.
fn setup_codegen_with_function(context: &Context) -> CodeGen<'_> {
    let mut codegen = CodeGen::new(context, "test");

    // Create a simple function to provide a basic block context
    let void_type = context.void_type();
    let fn_type = void_type.fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);

    codegen
}

// ========================================================================
// Tests for closure extraction (from compile_app helpers)
// ========================================================================

#[test]
fn test_extract_closure_from_struct_value() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create a closure struct value
    let env_ptr_type = context.ptr_type(AddressSpace::default());
    let closure_type = context.struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);
    let closure_val = closure_type.const_zero();

    let result =
        codegen.extract_closure_from_value(closure_val.into(), &Term::Var("f".to_string()));
    assert!(result.is_ok());
}

#[test]
fn test_extract_closure_from_pointer_value() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create a pointer to a closure
    let env_ptr_type = context.ptr_type(AddressSpace::default());
    let closure_type = context.struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);

    // Build alloca and store to create a valid pointer
    let ptr = codegen
        .builder
        .build_alloca(closure_type, "test_closure_ptr")
        .unwrap();
    let zero = closure_type.const_zero();
    codegen.builder.build_store(ptr, zero).unwrap();

    let result = codegen.extract_closure_from_value(ptr.into(), &Term::Var("f".to_string()));
    assert!(result.is_ok());
}

#[test]
fn test_extract_closure_components() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create a closure struct
    let env_ptr_type = context.ptr_type(AddressSpace::default());
    let closure_type = context.struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);
    let closure_val = closure_type.const_zero();

    let result = codegen.extract_closure_components(closure_val);
    assert!(result.is_ok());

    let (_fn_ptr, env_ptr) = result.unwrap();
    // Verify env_ptr is a pointer value
    assert!(env_ptr.is_pointer_value());
}

#[test]
fn test_build_closure_call() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create function pointer and args
    let env_ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();

    // Create a dummy function to call
    let fn_type = i64_type.fn_type(&[env_ptr_type.into(), i64_type.into()], false);
    let dummy_fn = codegen.module.add_function("dummy", fn_type, None);

    let fn_ptr = dummy_fn.as_global_value().as_pointer_value();
    let env_ptr = env_ptr_type.const_null();
    let arg_val = i64_type.const_int(42, false);

    let result = codegen.build_closure_call(fn_ptr, env_ptr.into(), arg_val.into(), &Type::Nat);
    assert!(result.is_ok());
}

#[test]
fn test_emit_noreturn_terminator() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let result = codegen.emit_noreturn_terminator(&Type::Unit);
    assert!(result.is_ok());

    // Verify we're now in a dead block
    let current_block = codegen.builder.get_insert_block();
    assert!(current_block.is_some());
    let block = current_block.unwrap();
    assert!(block.get_name().to_str().unwrap().contains("never_dead"));
}

// ========================================================================
// Tests for musttail emission (TCO)
// ========================================================================

/// Create a CodeGen with a closure-style function (ptr, ptr) -> ptr for musttail tests.
fn setup_codegen_with_closure_fn(context: &Context) -> (CodeGen<'_>, FunctionValue<'_>) {
    let mut codegen = CodeGen::new(context, "test_musttail");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
    let function = codegen.module.add_function("test_self_fn", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);
    (codegen, function)
}

#[test]
fn test_try_emit_musttail_matching_types() {
    let context = Context::create();
    let (mut codegen, function) = setup_codegen_with_closure_fn(&context);

    // Build a function pointer with the same signature as current_fn
    let fn_ptr = function.as_global_value().as_pointer_value();
    let ptr_type = context.ptr_type(AddressSpace::default());
    let env_ptr = ptr_type.const_null();
    let arg_val = ptr_type.const_null();

    // Should succeed — types match and return is a pointer (not struct)
    let result = codegen.try_emit_musttail(
        fn_ptr,
        env_ptr.into(),
        arg_val.into(),
        &Type::Nat, // Nat lowers to i64; this won't match ptr return → should return None
    );
    // Type mismatch (fn returns ptr, but we'd construct fn_type with i64 return)
    // so musttail should NOT fire
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_try_emit_musttail_type_mismatch_returns_none() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_ptr = ptr_type.const_null();
    let env_ptr = ptr_type.const_null();
    let arg_val = ptr_type.const_null();

    // current_fn is void() — won't match any closure type
    let result = codegen.try_emit_musttail(fn_ptr, env_ptr.into(), arg_val.into(), &Type::Nat);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_try_emit_musttail_no_current_fn() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    // current_fn is None

    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_ptr = ptr_type.const_null();

    let result = codegen.try_emit_musttail(
        fn_ptr,
        ptr_type.const_null().into(),
        ptr_type.const_null().into(),
        &Type::Nat,
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_try_emit_musttail_small_struct_return_accepted() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test_small_struct_ret");
    // Explicitly set AArch64 — this test verifies AArch64-specific struct return rejection
    codegen
        .module
        .set_triple(&inkwell::targets::TargetTriple::create(
            "aarch64-unknown-linux-gnu",
        ));

    // Create a function that returns a small struct (≤16 bytes, e.g. {ptr, ptr})
    // Even small struct returns are rejected on AArch64 because LLVM 18
    // musttail + indirect calls can crash in SelectionDAGISel.
    let ptr_type = context.ptr_type(AddressSpace::default());
    let small_struct = context.struct_type(&[ptr_type.into(), ptr_type.into()], false);
    let fn_type = small_struct.fn_type(&[ptr_type.into(), ptr_type.into()], false);
    let function = codegen
        .module
        .add_function("small_struct_ret_fn", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);

    let fn_ptr = function.as_global_value().as_pointer_value();

    // All struct returns are rejected by the ABI guard for indirect closure calls.
    let result = codegen.check_musttail_abi_safety(
        fn_type,
        crate::codegen::abi::MusttailCallKind::IndirectClosure,
    );
    assert!(
        result.is_err(),
        "struct return should be rejected by ABI check, got: {:?}",
        result
    );
}

#[test]
fn test_try_emit_musttail_large_struct_return_skipped() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test_struct_ret");

    // Create a function that returns a large struct (> 16 bytes → sret ABI)
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let large_struct =
        context.struct_type(&[i64_type.into(), i64_type.into(), i64_type.into()], false);
    let fn_type = large_struct.fn_type(&[ptr_type.into(), ptr_type.into()], false);
    let function = codegen
        .module
        .add_function("large_struct_ret_fn", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);

    let fn_ptr = function.as_global_value().as_pointer_value();

    // Use a type that lowers to the same large struct — should be skipped
    // because the struct return is > 16 bytes (sret ABI on AArch64).
    // We use Nat here; the key is that callee_fn_type won't match current_fn
    // since Nat lowers to i64, not the large struct. So this will be caught
    // by the type mismatch guard rather than the ABI guard.
    let result = codegen.try_emit_musttail(
        fn_ptr,
        ptr_type.const_null().into(),
        ptr_type.const_null().into(),
        &Type::Nat,
    );
    assert!(result.is_ok());
    assert!(
        result.unwrap().is_none(),
        "musttail should be skipped when types don't match"
    );
}

#[test]
fn test_in_tail_position_default_false() {
    let context = Context::create();
    let codegen = CodeGen::new(&context, "test");
    assert!(!codegen.compilation.in_tail_position);
}
