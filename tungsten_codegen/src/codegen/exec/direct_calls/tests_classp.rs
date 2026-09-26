//! Tests for Class-P indirect struct-parameter lowering (ADR 1.7.26e P2–P5).
//!
//! A non-flattenable struct param is passed by a caller-owned buffer `ptr`: the
//! `$direct` shim buffers the incoming by-value struct, `$direct_mt` reads it
//! through the buffer, and the self-tail edge stores the next value into the
//! buffer in place and `musttail`-forwards the same pointer (O(1) stack).

use super::decompose::ParamLowering;
use super::*;
use crate::codegen::CodeGen;
use inkwell::context::Context;
use inkwell::targets::TargetTriple;
use inkwell::AddressSpace;

/// Build a nested (non-flattenable) struct type `{ptr, {i64,i64}}`.
fn nested_struct(ctx: &Context) -> inkwell::types::StructType<'_> {
    let ptr = ctx.ptr_type(AddressSpace::default());
    let i64_ty = ctx.i64_type();
    let inner = ctx.struct_type(&[i64_ty.into(), i64_ty.into()], false);
    ctx.struct_type(&[ptr.into(), inner.into()], false)
}

/// A nested-struct param is not *decomposed* (18.5.26a), but under ADR 1.7.26e
/// it is now passed **indirect** (Class P): a `$direct_mt` is created with a
/// leading `ptr` buffer for the non-flattenable param instead of SKIPping.
#[test]
fn test_nested_struct_param_goes_indirect() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_nested_indirect");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let nested = nested_struct(&context);
    // i64 @nested$direct(ptr env, {ptr, {i64,i64}} nested) — scalar return, no sret.
    let fn_type = i64_type.fn_type(&[ptr_type.into(), nested.into()], false);
    cg.module.add_function("nested$direct", fn_type, None);

    let result = cg.declare_decomposed_entry("nested", fn_type).unwrap();
    assert_eq!(
        result,
        Some(vec![ParamLowering::Indirect]),
        "nested struct param is passed indirect (Class P)"
    );
    let mt_fn = cg
        .module
        .get_function("nested$direct_mt")
        .expect("Class-P $direct_mt should be created for a non-flattenable param");
    // No sret (scalar return); leading `ptr` buffer + env = 2 params, i64 return.
    assert_eq!(mt_fn.count_params(), 2, "indirect buffer + env");
    assert!(
        mt_fn.get_type().get_return_type().is_some(),
        "scalar-return Class-P $direct_mt keeps its i64 return (no sret)"
    );
    let ir = cg.module.print_to_string().to_string();
    let mt_line = ir
        .lines()
        .find(|l| l.contains(r#"@"nested$direct_mt""#))
        .expect("$direct_mt in IR");
    assert!(
        !mt_line.contains('{'),
        "indirect $direct_mt must carry no by-value struct, got: {mt_line}"
    );
}

/// ADR 1.7.26e (P4): the `$direct` shim for a pure Class-P function allocates a
/// buffer for the incoming by-value non-flattenable struct, stores it, and passes
/// the `ptr` to `$direct_mt` — bridging the by-value ABI to the indirect one.
#[test]
fn test_class_p_shim_buffers_indirect_param() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_cp_shim");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let nested = nested_struct(&context);
    // i64 @cp$direct(ptr env, {ptr,{i64,i64}} ctx) — scalar return, non-flattenable param.
    let fn_type = i64_type.fn_type(&[ptr_type.into(), nested.into()], false);
    cg.module.add_function("cp$direct", fn_type, None);

    let param_map = cg.declare_decomposed_entry("cp", fn_type).unwrap().unwrap();
    assert_eq!(param_map, vec![ParamLowering::Indirect]);
    let mt_fn = cg.module.get_function("cp$direct_mt").unwrap();
    cg.compile_decompose_shim("cp", "cp$direct_mt", mt_fn, &param_map)
        .unwrap();

    assert!(cg.module.verify().is_ok(), "shim IR must verify");
    let ir = cg.module.print_to_string().to_string();
    // Shim allocates a buffer, stores the incoming struct, and calls $direct_mt
    // with a `ptr` (not the by-value struct).
    assert!(ir.contains("alloca"), "shim allocates an indirect buffer");
    assert!(
        ir.contains("store"),
        "shim stores the incoming struct into the buffer"
    );
    assert!(
        ir.contains(r#"call i64 @"cp$direct_mt"(ptr"#),
        "shim calls $direct_mt with a leading ptr buffer, got IR:\n{ir}"
    );
}

/// ADR 1.7.26e (sret + indirect combined): a Class-P function with BOTH a
/// by-value struct return AND a non-flattenable param lowers `$direct_mt` to
/// `void @f(ptr sret, ptr buf, ptr env)`; the self-tail edge forwards the sret
/// out-ptr + stores the next aggregate into the buffer + `musttail call void` +
/// `ret void` — verifier-clean, no bounce.
#[test]
fn test_class_p_sret_plus_indirect_edge() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_cp_sret");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let nested = nested_struct(&context);
    let ret_struct = context.struct_type(&[i64_type.into(), ptr_type.into()], false);
    // {i64,ptr} @sr$direct(ptr env, {ptr,{i64,i64}} ctx) — struct return + indirect param.
    let fn_type = ret_struct.fn_type(&[ptr_type.into(), nested.into()], false);
    cg.module.add_function("sr$direct", fn_type, None);
    let param_map = cg.declare_decomposed_entry("sr", fn_type).unwrap().unwrap();
    assert_eq!(param_map, vec![ParamLowering::Indirect]);

    let mt_fn = cg.module.get_function("sr$direct_mt").unwrap();
    // Layout: [0]=sret out-ptr, [1]=ctx buffer, [2]=env → void return, 3 params.
    assert!(
        mt_fn.get_type().get_return_type().is_none(),
        "sret+indirect $direct_mt returns void"
    );
    assert_eq!(mt_fn.count_params(), 3, "sret + indirect buffer + env");

    cg.direct_calls.set_decompose_map("sr", param_map);
    cg.compilation.current_fn = Some(mt_fn);
    let entry = context.append_basic_block(mt_fn, "entry");
    cg.builder.position_at_end(entry);
    let next_ctx = nested.get_undef().into();
    let emitted = cg.try_emit_decomposed_musttail("sr", &[next_ctx]).unwrap();
    assert!(emitted.is_some(), "sret+indirect self edge must EMIT");
    // Void (sret) entry: terminate the epilogue's dead block with `ret void`
    // (the real flow uses emit_sret_return; here a bare void return suffices).
    cg.builder.build_return(None).unwrap();

    assert!(cg.module.verify().is_ok(), "sret+indirect IR must verify");
    let ir = cg.module.print_to_string().to_string();
    assert!(
        ir.contains("store") && ir.contains(r#"musttail call void @"sr$direct_mt"(ptr"#),
        "sret+indirect edge: store next + musttail call void forwarding sret+buffer, got:\n{ir}"
    );
}

/// ADR 1.7.26e (P5, the O(1) crux): inside a pure Class-P `$direct_mt`, the
/// self-recursive tail edge stores the next aggregate into the incoming buffer
/// in place and `musttail`-forwards the SAME pointer — no bounce, verifier-clean.
#[test]
fn test_class_p_tail_edge_stores_and_forwards_buffer() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_cp_edge");
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let nested = nested_struct(&context);
    let fn_type = i64_type.fn_type(&[ptr_type.into(), nested.into()], false);
    cg.module.add_function("cp$direct", fn_type, None);
    let param_map = cg.declare_decomposed_entry("cp", fn_type).unwrap().unwrap();
    let mt_fn = cg.module.get_function("cp$direct_mt").unwrap();
    // $direct_mt signature: i64 (ptr buf, ptr env) — no sret, indirect run leads.
    assert_eq!(mt_fn.count_params(), 2);

    // Drive the self-recursive tail edge from inside `$direct_mt`.
    cg.direct_calls.set_decompose_map("cp", param_map);
    cg.compilation.current_fn = Some(mt_fn);
    let entry = context.append_basic_block(mt_fn, "entry");
    cg.builder.position_at_end(entry);

    let next_ctx = nested.get_undef().into();
    let emitted = cg.try_emit_decomposed_musttail("cp", &[next_ctx]).unwrap();
    assert!(
        emitted.is_some(),
        "Class-P self edge is musttail-legal → must EMIT"
    );
    // The epilogue leaves the builder in an unreachable dead block; terminate it
    // as the real compile flow does (emit_return_if_needed) so the module verifies.
    cg.emit_return_if_needed(&emitted.unwrap()).unwrap();

    assert!(
        cg.module.verify().is_ok(),
        "Class-P tail-edge IR must verify"
    );
    let ir = cg.module.print_to_string().to_string();
    // Store the next aggregate into the incoming buffer, then musttail-forward it.
    assert!(
        ir.contains("store"),
        "tail edge stores the next aggregate into the buffer, got IR:\n{ir}"
    );
    assert!(
        ir.contains(r#"musttail call i64 @"cp$direct_mt"(ptr"#),
        "tail edge musttail-forwards the buffer ptr, got IR:\n{ir}"
    );
    // No bounce through the `$direct` shim.
    let bounced = ir.lines().any(|l| {
        let t = l.trim_start();
        (t.starts_with("call") || t.starts_with("musttail call") || t.starts_with("tail call"))
            && t.contains(r#"@"cp$direct"("#)
    });
    assert!(
        !bounced,
        "tail edge must not bounce through the $direct shim"
    );
}
