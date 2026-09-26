//! Tests for argument decomposition (ADR 18.5.26a).

use super::decompose::ParamLowering;
use super::*;
use crate::codegen::CodeGen;
use inkwell::context::Context;
use inkwell::targets::TargetTriple;
use inkwell::AddressSpace;

#[test]
fn test_declare_decomposed_entry_creates_mt_function() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_declare_mt");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let string_struct = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    // fn_type: i64 @foo$direct(ptr env, {ptr, i64} s, i64 n)
    let fn_type = i64_type.fn_type(
        &[ptr_type.into(), string_struct.into(), i64_type.into()],
        false,
    );
    cg.module.add_function("foo$direct", fn_type, None);

    let result = cg.declare_decomposed_entry("foo", fn_type).unwrap();
    assert!(result.is_some(), "should declare decomposed entry");
    let param_map = result.unwrap();
    // First param (after env) is struct with 2 fields, second is scalar
    assert_eq!(
        param_map,
        vec![ParamLowering::Decompose(2), ParamLowering::Passthrough]
    );

    // $direct_mt should exist in the module
    let mt_fn = cg.module.get_function("foo$direct_mt");
    assert!(mt_fn.is_some(), "$direct_mt should be declared");
    let mt_fn = mt_fn.unwrap();
    // $direct_mt params: ptr env, ptr s.0, i64 s.1, i64 n = 4 params
    assert_eq!(mt_fn.count_params(), 4);
}

#[test]
fn test_declare_decomposed_entry_skips_no_struct_params() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_declare_mt_no_struct");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[ptr_type.into(), i64_type.into()], false);
    cg.module.add_function("bar$direct", fn_type, None);

    let result = cg.declare_decomposed_entry("bar", fn_type).unwrap();
    assert!(result.is_none(), "no struct params → no decomposition");
    assert!(
        cg.module.get_function("bar$direct_mt").is_none(),
        "$direct_mt should not be declared"
    );
}

#[test]
fn test_shim_has_correct_structure() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_shim");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let string_struct = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    // fn_type: i64 @baz$direct(ptr env, {ptr, i64} s, i64 n)
    let fn_type = i64_type.fn_type(
        &[ptr_type.into(), string_struct.into(), i64_type.into()],
        false,
    );
    let direct_fn = cg.module.add_function("baz$direct", fn_type, None);
    let param_map = vec![ParamLowering::Decompose(2), ParamLowering::Passthrough];

    // Declare the $direct_mt entry
    let result = cg.declare_decomposed_entry("baz", fn_type).unwrap();
    assert!(result.is_some());
    let mt_fn = cg.module.get_function("baz$direct_mt").unwrap();

    // Compile the shim
    let shim_result = cg.compile_decompose_shim("baz", "baz$direct_mt", mt_fn, &param_map);
    assert!(shim_result.is_ok(), "shim compilation should succeed");

    // Verify the $direct function has an entry block with a terminator
    let entry_bb = direct_fn.get_first_basic_block();
    assert!(entry_bb.is_some(), "shim should have entry block");
    assert!(
        entry_bb.unwrap().get_terminator().is_some(),
        "shim entry block should have terminator"
    );
}

/// AC9: Shim IR extracts struct fields and calls $direct_mt with scalars.
#[test]
fn test_shim_ir_extracts_and_delegates() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_shim_ir");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let string_struct = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    let fn_type = i64_type.fn_type(
        &[ptr_type.into(), string_struct.into(), i64_type.into()],
        false,
    );
    cg.module.add_function("shim_test$direct", fn_type, None);
    let param_map = vec![ParamLowering::Decompose(2), ParamLowering::Passthrough];

    cg.declare_decomposed_entry("shim_test", fn_type).unwrap();
    let mt_fn = cg.module.get_function("shim_test$direct_mt").unwrap();
    cg.compile_decompose_shim("shim_test", "shim_test$direct_mt", mt_fn, &param_map)
        .unwrap();

    let ir = cg.module.print_to_string().to_string();
    // Shim must extractvalue the struct param fields
    assert!(
        ir.contains("extractvalue"),
        "shim should extractvalue struct fields"
    );
    // Shim must call $direct_mt (LLVM quotes names containing $)
    assert!(
        ir.contains(r#"call i64 @"shim_test$direct_mt""#),
        "shim should call $direct_mt, got IR:\n{}",
        ir
    );
    // $direct_mt should have flattened params (ptr, ptr, i64, i64)
    assert!(
        ir.contains(r#"@"shim_test$direct_mt"(ptr"#),
        "$direct_mt should be declared with scalar params"
    );
}

/// AC10: Decomposed $direct_mt has scalar params, not struct params.
#[test]
fn test_decomposed_mt_has_scalar_params_in_ir() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_mt_ir");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let string_struct = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    // {ptr, i64} param → should decompose to (ptr, i64) scalars
    let fn_type = i64_type.fn_type(&[ptr_type.into(), string_struct.into()], false);
    cg.module.add_function("scalar_test$direct", fn_type, None);
    cg.declare_decomposed_entry("scalar_test", fn_type).unwrap();

    let ir = cg.module.print_to_string().to_string();
    // $direct_mt should NOT have { ptr, i64 } in its signature
    let mt_line = ir
        .lines()
        .find(|l| l.contains(r#"@"scalar_test$direct_mt""#))
        .expect("$direct_mt should appear in IR");
    assert!(
        !mt_line.contains("{ ptr, i64 }"),
        "$direct_mt should not have struct params, got: {}",
        mt_line
    );
    // Should have scalar ptr and i64 instead
    assert!(
        mt_line.contains("ptr") && mt_line.contains("i64"),
        "$direct_mt should have scalar ptr and i64 params, got: {}",
        mt_line
    );
}

/// ADR 1.7.26a (Stage 1): a struct-*returning* fn with a flattenable struct
/// param lowers `$direct_mt` to the sret ABI — `void` return, leading `ptr`
/// out-pointer — so the gate no longer SKIPs on the struct return.
#[test]
fn test_sret_direct_mt_has_void_return_and_leading_out_pointer() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_sret_shape");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    // By-value aggregate RETURN {i64, ptr} + flattenable {ptr, i64} struct param.
    let ret_struct = context.struct_type(&[i64_type.into(), ptr_type.into()], false);
    let string_struct = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    // {i64,ptr} @f$direct(ptr env, {ptr,i64} s)
    let fn_type = ret_struct.fn_type(&[ptr_type.into(), string_struct.into()], false);
    cg.module.add_function("f$direct", fn_type, None);

    let param_map = cg.declare_decomposed_entry("f", fn_type).unwrap().unwrap();
    assert_eq!(param_map, vec![ParamLowering::Decompose(2)]);

    let mt_fn = cg.module.get_function("f$direct_mt").unwrap();
    // sret: `$direct_mt` returns void (LLVM 18 rejects musttail with struct return).
    assert!(
        mt_fn.get_type().get_return_type().is_none(),
        "sret $direct_mt must return void, got {:?}",
        mt_fn.get_type().get_return_type()
    );
    // Params: sret out-ptr, env, s.0 (ptr), s.1 (i64) = 4.
    assert_eq!(
        mt_fn.count_params(),
        4,
        "sret out-ptr + env + 2 flattened scalars"
    );
}

/// ADR 1.7.26a (Stage 1) — the core regression guard: inside an sret-style
/// `$direct_mt`, the self-recursive tail edge is `musttail call void @…$direct_mt`
/// forwarding the out-pointer, with **no** bounce through the `$direct` shim.
#[test]
fn test_sret_direct_mt_recursive_edge_is_musttail_void_no_bounce() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_sret_edge");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let ret_struct = context.struct_type(&[i64_type.into(), ptr_type.into()], false);
    let string_struct = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    let fn_type = ret_struct.fn_type(&[ptr_type.into(), string_struct.into()], false);
    cg.module.add_function("f$direct", fn_type, None);
    let param_map = cg.declare_decomposed_entry("f", fn_type).unwrap().unwrap();
    let mt_fn = cg.module.get_function("f$direct_mt").unwrap();

    // Drive the self-recursive tail edge from inside `$direct_mt`.
    cg.direct_calls.set_decompose_map("f", param_map);
    cg.compilation.current_fn = Some(mt_fn);
    let entry = context.append_basic_block(mt_fn, "entry");
    cg.builder.position_at_end(entry);

    // Recursive call passes the (unchanged-shape) struct arg; undef suffices for IR.
    let arg = string_struct.get_undef().into();
    let emitted = cg.try_emit_decomposed_musttail("f", &[arg]).unwrap();
    assert!(
        emitted.is_some(),
        "sret $direct_mt is musttail-legal (void return) → recursive edge must EMIT"
    );

    let ir = cg.module.print_to_string().to_string();
    // Recursive edge: `musttail call void @"f$direct_mt"(ptr …)` — forwards the sret
    // out-pointer, void return.
    assert!(
        ir.contains(r#"musttail call void @"f$direct_mt""#),
        "recursive edge must be `musttail call void @f$direct_mt`, got IR:\n{ir}"
    );
    // No `$direct_mt ⇄ $direct` bounce: no call/invoke instruction targets the
    // `$direct` shim (the `declare @"f$direct"(…)` line is not a call).
    let bounced = ir.lines().any(|l| {
        let t = l.trim_start();
        (t.starts_with("call") || t.starts_with("musttail call") || t.starts_with("tail call"))
            && t.contains(r#"@"f$direct"("#)
    });
    assert!(
        !bounced,
        "recursive edge must not bounce through the $direct shim, got IR:\n{ir}"
    );
}

/// Mixed struct + scalar ordering produces correct param_map positions.
#[test]
fn test_declare_mixed_struct_scalar_param_map() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_mixed_map");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let string_struct = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    // fn(env, struct, i64, struct) — interleaved struct + scalar
    let fn_type = i64_type.fn_type(
        &[
            ptr_type.into(),
            string_struct.into(),
            i64_type.into(),
            string_struct.into(),
        ],
        false,
    );
    cg.module.add_function("mixed$direct", fn_type, None);

    let result = cg.declare_decomposed_entry("mixed", fn_type).unwrap();
    assert!(result.is_some());
    let param_map = result.unwrap();
    // param_map: [Some(2), None, Some(2)] — struct, scalar, struct
    assert_eq!(
        param_map,
        vec![
            ParamLowering::Decompose(2),
            ParamLowering::Passthrough,
            ParamLowering::Decompose(2)
        ]
    );

    let mt_fn = cg.module.get_function("mixed$direct_mt").unwrap();
    // env + ptr + i64 + i64 + ptr + i64 = 6 params
    assert_eq!(mt_fn.count_params(), 6);
}

/// ADR 1.7.26c P1 (Class R): when no struct params are decomposable but the
/// return is struct (sret-eligible), declare_decomposed_entry still creates
/// a `$direct_mt` entry with sret and non-decomposed params.
/// This unblocks musttail for return-only struct functions (5 functions).
#[test]
fn test_class_r_direct_mt_with_sret_when_no_decomposition() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_class_r");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();

    // Struct-returning function with scalar-only params (e.g., module_map_lookup).
    // Return: {i64, ptr} (non-recursive sum-with-payload).
    // Params: i64 (scalar), i64 (scalar) → no struct params, decomposition not possible,
    // but return is struct → Class R, should create $direct_mt with sret.
    let ret_struct = context.struct_type(&[i64_type.into(), ptr_type.into()], false);
    // fn_type: {i64,ptr} @foo$direct(ptr env, i64 key, i64 sentinel)
    let fn_type = ret_struct.fn_type(&[ptr_type.into(), i64_type.into(), i64_type.into()], false);

    cg.module.add_function("foo$direct", fn_type, None);

    // Test: declare_decomposed_entry should create $direct_mt with sret
    let result = cg.declare_decomposed_entry("foo", fn_type).unwrap();
    assert!(
        result.is_some(),
        "Class R should declare $direct_mt even with no decomposable params"
    );

    let param_map = result.unwrap();
    // param_map: [None, None] — both scalar params are passthrough (no decomposition)
    assert_eq!(
        param_map,
        vec![ParamLowering::Passthrough, ParamLowering::Passthrough]
    );

    let mt_fn = cg.module.get_function("foo$direct_mt").unwrap();
    // Verify sret: $direct_mt returns void (LLVM 18 rejects musttail with struct return)
    assert!(
        mt_fn.get_type().get_return_type().is_none(),
        "Class R $direct_mt must return void (sret)"
    );
    // Params: sret out-ptr, env, i64 key, i64 sentinel = 4 params
    assert_eq!(
        mt_fn.count_params(),
        4,
        "Class R $direct_mt has: sret out-ptr + env + 2 scalar params (no decomposition)"
    );
}
