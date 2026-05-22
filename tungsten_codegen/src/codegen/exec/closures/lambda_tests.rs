use super::*;
use crate::codegen::CodeGen;
use inkwell::context::Context;

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
// Tests for capture info building
// ========================================================================

#[test]
fn test_build_capture_info_empty() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    let capture_info = codegen.build_capture_info(&[]);
    assert!(capture_info.names.is_empty());
    assert!(capture_info.field_types.is_empty());
}

#[test]
fn test_build_capture_info_with_variables() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Add variables to environment
    let i64_val = context.i64_type().const_int(42, false);
    codegen
        .compilation
        .env
        .insert("x".to_string(), (i64_val.into(), Type::Nat));
    codegen
        .compilation
        .env
        .insert("y".to_string(), (i64_val.into(), Type::Nat));

    let capture_info = codegen.build_capture_info(&["x".to_string(), "y".to_string()]);
    assert_eq!(capture_info.names.len(), 2);
    assert_eq!(capture_info.field_types.len(), 2);
}

// ========================================================================
// Tests for lambda function creation
// ========================================================================

#[test]
fn test_create_lambda_function() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let lambda_fn = codegen.create_lambda_function(&Type::Nat, &Type::Bool);

    // Verify function signature: (ptr, i64) -> i1
    let fn_type = lambda_fn.get_type();
    assert_eq!(fn_type.get_param_types().len(), 2);
}

// ========================================================================
// Tests for state save/restore
// ========================================================================

#[test]
fn test_save_and_restore_lambda_state() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Add something to environment
    let i64_val = context.i64_type().const_int(42, false);
    codegen
        .compilation
        .env
        .insert("test_var".to_string(), (i64_val.into(), Type::Nat));
    let original_fn = codegen.compilation.current_fn;

    // Save state
    let saved = codegen.save_lambda_state();
    assert!(saved.env.contains_key("test_var"));

    // Modify state
    codegen.compilation.env.clear();
    codegen.compilation.current_fn = None;

    // Restore
    codegen.restore_lambda_state(saved);
    assert!(codegen.compilation.env.contains_key("test_var"));
    assert_eq!(codegen.compilation.current_fn, original_fn);
}

// ========================================================================
// Tests for environment allocation
// ========================================================================

#[test]
fn test_allocate_lambda_environment_empty_returns_null() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let capture_info = codegen.build_capture_info(&[]);
    let result = codegen.allocate_lambda_environment(&capture_info);

    assert!(result.is_ok());
    let env_ptr = result.unwrap();
    assert!(env_ptr.is_null());
}

// ========================================================================
// Tests for closure struct building
// ========================================================================

#[test]
fn test_build_closure_struct() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create a dummy lambda function
    let env_ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[env_ptr_type.into(), i64_type.into()], false);
    let lambda_fn = codegen.module.add_function("test_lambda", fn_type, None);

    let env_ptr = env_ptr_type.const_null();
    let result = codegen.build_closure_struct(lambda_fn, env_ptr);

    assert!(result.is_ok());
    let closure = result.unwrap();
    assert!(closure.is_struct_value());
}
