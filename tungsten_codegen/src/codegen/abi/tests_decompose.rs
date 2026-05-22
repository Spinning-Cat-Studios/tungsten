//! Tests for struct decomposition eligibility (ADR 18.5.26a).

use super::*;
use inkwell::context::Context;
use inkwell::targets::TargetTriple;
use inkwell::AddressSpace;

fn codegen_aarch64<'ctx>(context: &'ctx Context, name: &str) -> CodeGen<'ctx> {
    let mut cg = CodeGen::new(context, name);
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    cg
}

// ========================================================================
// check_decomposition_eligible
// ========================================================================

#[test]
fn test_decompose_eligible_flat_struct_param() {
    let context = Context::create();
    let cg = codegen_aarch64(&context, "test_decompose_flat");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    // String-like struct: { ptr, i64 }
    let string_struct = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    let fn_type = i64_type.fn_type(
        &[ptr_type.into(), string_struct.into(), i64_type.into()],
        false,
    );
    let result = cg.check_decomposition_eligible(fn_type);
    assert!(result.is_some(), "flat struct should be eligible");
    let flattened = result.unwrap();
    // env ptr skipped: {ptr, i64}, i64 → ptr, i64, i64
    assert_eq!(flattened.len(), 3);
}

#[test]
fn test_decompose_eligible_no_struct_params() {
    let context = Context::create();
    let cg = codegen_aarch64(&context, "test_decompose_no_struct");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[ptr_type.into(), i64_type.into()], false);
    let result = cg.check_decomposition_eligible(fn_type);
    assert!(result.is_none(), "no struct params → nothing to decompose");
}

#[test]
fn test_decompose_ineligible_nested_struct() {
    let context = Context::create();
    let cg = codegen_aarch64(&context, "test_decompose_nested");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let inner = context.struct_type(&[i64_type.into(), i64_type.into()], false);
    // Nested struct: { ptr, { i64, i64 } }
    let outer = context.struct_type(&[ptr_type.into(), inner.into()], false);
    let fn_type = i64_type.fn_type(&[ptr_type.into(), outer.into()], false);
    let result = cg.check_decomposition_eligible(fn_type);
    assert!(result.is_none(), "nested struct should be ineligible");
}

#[test]
fn test_decompose_ineligible_array_field() {
    let context = Context::create();
    let cg = codegen_aarch64(&context, "test_decompose_array");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let arr = i64_type.array_type(4);
    let struct_with_arr = context.struct_type(&[ptr_type.into(), arr.into()], false);
    let fn_type = i64_type.fn_type(&[ptr_type.into(), struct_with_arr.into()], false);
    let result = cg.check_decomposition_eligible(fn_type);
    assert!(
        result.is_none(),
        "struct with array field should be ineligible"
    );
}

#[test]
fn test_decompose_ineligible_too_many_fields() {
    let context = Context::create();
    let cg = codegen_aarch64(&context, "test_decompose_many_fields");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    // 9 fields → exceeds limit of 8
    let big = context.struct_type(
        &[
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
        ],
        false,
    );
    let fn_type = i64_type.fn_type(&[ptr_type.into(), big.into()], false);
    let result = cg.check_decomposition_eligible(fn_type);
    assert!(
        result.is_none(),
        "struct with >8 fields should be ineligible"
    );
}

#[test]
fn test_decompose_eligible_multiple_struct_params() {
    let context = Context::create();
    let cg = codegen_aarch64(&context, "test_decompose_multi");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let s1 = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    let s2 = context.struct_type(&[i64_type.into(), i64_type.into()], false);
    // Two struct params: {ptr, i64} and {i64, i64}
    let fn_type = i64_type.fn_type(&[ptr_type.into(), s1.into(), s2.into()], false);
    let result = cg.check_decomposition_eligible(fn_type);
    assert!(
        result.is_some(),
        "two flat struct params should be eligible"
    );
    let flattened = result.unwrap();
    // env skipped: ptr+i64 (s1), i64+i64 (s2) = 4
    assert_eq!(flattened.len(), 4);
}

#[test]
fn test_decompose_eligible_exact_8_fields() {
    let context = Context::create();
    let cg = codegen_aarch64(&context, "test_decompose_8_fields");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    // Exactly 8 fields → at the limit, should be eligible
    let s = context.struct_type(
        &[
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
            i64_type.into(),
        ],
        false,
    );
    let fn_type = i64_type.fn_type(&[ptr_type.into(), s.into()], false);
    let result = cg.check_decomposition_eligible(fn_type);
    assert!(
        result.is_some(),
        "struct with exactly 8 fields should be eligible"
    );
}

#[test]
fn test_decompose_mixed_struct_scalar_ordering() {
    let context = Context::create();
    let cg = codegen_aarch64(&context, "test_decompose_mixed");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let s = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    // fn(env, struct, i64, struct) — interleaved struct + scalar
    let fn_type = i64_type.fn_type(
        &[ptr_type.into(), s.into(), i64_type.into(), s.into()],
        false,
    );
    let result = cg.check_decomposition_eligible(fn_type);
    assert!(result.is_some(), "mixed struct+scalar should be eligible");
    let flattened = result.unwrap();
    // env skipped: ptr+i64 (s), i64, ptr+i64 (s) = 5 scalars
    assert_eq!(flattened.len(), 5);
}
