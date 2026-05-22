use super::*;
use crate::codegen::CodeGen;
use inkwell::context::Context;

/// Create a CodeGen instance with an active function and positioned builder.
fn setup_codegen_with_function(context: &Context) -> CodeGen<'_> {
    let mut codegen = CodeGen::new(context, "test");

    let void_type = context.void_type();
    let fn_type = void_type.fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);

    codegen
}

// ========================================================================
// Tests for arrow type extraction
// ========================================================================

#[test]
fn test_extract_fix_arrow_type_valid() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    let arrow_ty = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool));
    let result = codegen.extract_fix_arrow_type(&arrow_ty);

    assert!(result.is_ok());
    let (param, ret) = result.unwrap();
    assert_eq!(param, Type::Nat);
    assert_eq!(ret, Type::Bool);
}

#[test]
fn test_extract_fix_arrow_type_nested() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    // (Nat -> Bool) -> Nat
    let inner = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool));
    let arrow_ty = Type::Arrow(Box::new(inner.clone()), Box::new(Type::Nat));
    let result = codegen.extract_fix_arrow_type(&arrow_ty);

    assert!(result.is_ok());
    let (param, ret) = result.unwrap();
    assert_eq!(param, inner);
    assert_eq!(ret, Type::Nat);
}

#[test]
fn test_extract_fix_arrow_type_invalid() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    let result = codegen.extract_fix_arrow_type(&Type::Nat);
    assert!(result.is_err());

    let result = codegen.extract_fix_arrow_type(&Type::Bool);
    assert!(result.is_err());

    let result = codegen.extract_fix_arrow_type(&Type::Unit);
    assert!(result.is_err());
}

// ========================================================================
// Tests for fix function creation
// ========================================================================

#[test]
fn test_create_fix_function() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let fix_fn = codegen.create_fix_function(&Type::Nat, &Type::Bool);

    // Verify function exists and has correct arity
    let fn_type = fix_fn.get_type();
    assert_eq!(fn_type.get_param_types().len(), 2); // env_ptr + param
}

#[test]
fn test_create_fix_function_unique_names() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let fix_fn1 = codegen.create_fix_function(&Type::Nat, &Type::Nat);
    let fix_fn2 = codegen.create_fix_function(&Type::Nat, &Type::Nat);

    // Names should be unique
    assert_ne!(
        fix_fn1.get_name().to_str().unwrap(),
        fix_fn2.get_name().to_str().unwrap()
    );
}

// ========================================================================
// Tests for closure struct type
// ========================================================================

#[test]
fn test_get_closure_struct_type() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    let closure_type = codegen.get_closure_struct_type();

    // Should have 2 fields (fn_ptr, env_ptr)
    assert_eq!(closure_type.get_field_types().len(), 2);
}

// ========================================================================
// Tests for self-reference closure building
// ========================================================================

#[test]
fn test_build_self_reference_closure() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create a dummy function
    let env_ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[env_ptr_type.into(), i64_type.into()], false);
    let fix_fn = codegen.module.add_function("test_fix", fn_type, None);

    let null_env = env_ptr_type.const_null();
    let result = codegen.build_self_reference_closure(fix_fn, null_env);

    assert!(result.is_ok());
    let closure = result.unwrap();
    // Verify it's a struct with 2 fields
    assert_eq!(closure.get_type().get_field_types().len(), 2);
}

// ========================================================================
// Tests for fix closure building
// ========================================================================

#[test]
fn test_build_fix_closure() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create a dummy function
    let env_ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[env_ptr_type.into(), i64_type.into()], false);
    let fix_fn = codegen.module.add_function("test_fix", fn_type, None);

    let null_env = env_ptr_type.const_null();
    let result = codegen.build_fix_closure(fix_fn, null_env);

    assert!(result.is_ok());
    assert!(result.unwrap().is_struct_value());
}

// ========================================================================
// Tests for fix lambda body validation
// ========================================================================

#[test]
fn test_compile_fix_lambda_body_non_lambda_error() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create a fix function
    let env_ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[env_ptr_type.into(), i64_type.into()], false);
    let fix_fn = codegen.module.add_function("test_fix", fn_type, None);

    // Set up the function context
    let entry = context.append_basic_block(fix_fn, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(fix_fn);

    // Try to compile a non-lambda body
    let non_lambda_body = Term::NatLit(42);
    let result = codegen.compile_fix_lambda_body(fix_fn, &Type::Nat, &non_lambda_body);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, CodeGenError::TypeError(_)));
}
