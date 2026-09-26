//! Tests for `cast_to_type`'s shrinking-aggregate guard (ADR 2.7.26b T1).
//!
//! A shrinking aggregate cast is the signature of the 1.7.26e §6.6
//! phi-poisoning miscompile; it must be a hard `CodeGenError`. Scalar and
//! widening casts keep the existing memcpy path.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::context::Context;

fn setup_codegen_with_function<'ctx>(context: &'ctx Context) -> CodeGen<'ctx> {
    let mut codegen = CodeGen::new(context, "cast_test");

    let void_type = context.void_type();
    let fn_type = void_type.fn_type(&[], false);
    let function = codegen.module.add_function("cast_test_fn", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);

    codegen
}

/// Loose context assertions (AC T1): the error names the enclosing function
/// and the aggregate kind — not asserted on exact wording.
fn assert_shrink_error(result: Result<inkwell::values::BasicValueEnum, CodeGenError>, kind: &str) {
    match result {
        Err(CodeGenError::TypeError(msg)) => {
            assert!(
                msg.contains("cast_test_fn"),
                "error should name the enclosing function, got: {msg}"
            );
            assert!(
                msg.contains(kind),
                "error should name the aggregate kind `{kind}`, got: {msg}"
            );
            assert!(msg.contains("bytes"), "error should name sizes, got: {msg}");
        }
        other => panic!("expected TypeError for shrinking aggregate cast, got: {other:?}"),
    }
}

#[test]
fn shrink_struct_to_scalar_is_error() {
    // The exact §6.6 shape: a {ptr, ptr} sret result cast down to an i1 dummy.
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let ptr_ty = context.ptr_type(inkwell::AddressSpace::default());
    let struct_ty = context.struct_type(&[ptr_ty.into(), ptr_ty.into()], false);
    let src = struct_ty.const_zero();

    let result = codegen.cast_to_type(src.into(), context.bool_type().into());
    assert_shrink_error(result, "struct");
}

#[test]
fn shrink_struct_to_smaller_struct_is_error() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let i64_ty = context.i64_type();
    let big = context.struct_type(&[i64_ty.into(), i64_ty.into()], false);
    let small = context.struct_type(&[context.i32_type().into()], false);

    let result = codegen.cast_to_type(big.const_zero().into(), small.into());
    assert_shrink_error(result, "struct");
}

#[test]
fn shrink_struct_to_empty_struct_is_error() {
    // The 3.7.26a shape: a payload-carrying ADT value ({ i32, [N x i8] })
    // cast down to a dead arm's `{}` unit placeholder. The guard must keep
    // rejecting this even with the dead-arm inference fix in place.
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let i8_ty = context.i8_type();
    let adt_like = context.struct_type(
        &[context.i32_type().into(), i8_ty.array_type(8).into()],
        false,
    );
    let empty = context.struct_type(&[], false);

    let result = codegen.cast_to_type(adt_like.const_zero().into(), empty.into());
    assert_shrink_error(result, "struct");
}

#[test]
fn shrink_array_is_error() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let arr_ty = context.i64_type().array_type(4);
    let result = codegen.cast_to_type(arr_ty.const_zero().into(), context.i64_type().into());
    assert_shrink_error(result, "array");
}

#[test]
fn widen_aggregate_is_ok() {
    // Widening an aggregate keeps the existing zero-fill memcpy path.
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let small = context.struct_type(&[context.i32_type().into()], false);
    let i64_ty = context.i64_type();
    let big = context.struct_type(&[i64_ty.into(), i64_ty.into()], false);

    let result = codegen.cast_to_type(small.const_zero().into(), big.into());
    assert!(
        result.is_ok(),
        "widening aggregate cast must stay Ok: {result:?}"
    );
}

#[test]
fn scalar_casts_unchanged() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Shrinking scalar↔scalar keeps the memcpy path (zero-fill semantics).
    let src = context.i64_type().const_int(42, false);
    let result = codegen.cast_to_type(src.into(), context.i32_type().into());
    assert!(result.is_ok(), "scalar shrink must stay Ok: {result:?}");

    // Widening scalar↔scalar likewise.
    let src = context.i32_type().const_int(7, false);
    let result = codegen.cast_to_type(src.into(), context.i64_type().into());
    assert!(result.is_ok(), "scalar widen must stay Ok: {result:?}");
}

#[test]
fn same_type_is_identity() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let src = context.i64_type().const_int(1, false);
    let result = codegen
        .cast_to_type(src.into(), context.i64_type().into())
        .unwrap();
    assert_eq!(
        result.into_int_value().get_zero_extended_constant(),
        Some(1)
    );
}
