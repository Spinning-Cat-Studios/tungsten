use super::*;
use inkwell::context::Context;
use inkwell::targets::TargetTriple;
use inkwell::AddressSpace;

/// Helper: create a CodeGen with an explicit AArch64 triple.
fn codegen_aarch64<'ctx>(context: &'ctx Context, name: &str) -> CodeGen<'ctx> {
    let mut cg = CodeGen::new(context, name);
    cg.module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    cg
}

// ========================================================================
// Tests for check_musttail_abi_safety (AArch64 rejection behavior)
// ========================================================================

#[test]
fn test_musttail_abi_safety_void_return() {
    let context = Context::create();
    let codegen = CodeGen::new(&context, "test");
    let ptr_type = context.ptr_type(AddressSpace::default());
    // void(ptr, ptr) — no return type at all
    let fn_type = context
        .void_type()
        .fn_type(&[ptr_type.into(), ptr_type.into()], false);
    assert!(codegen
        .check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure)
        .is_ok());
}

#[test]
fn test_musttail_abi_safety_ptr_only() {
    let context = Context::create();
    let codegen = CodeGen::new(&context, "test");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
    assert!(codegen
        .check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure)
        .is_ok());
}

#[test]
fn test_musttail_abi_safety_i64_return() {
    let context = Context::create();
    let codegen = CodeGen::new(&context, "test");
    let i64_type = context.i64_type();
    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_type = i64_type.fn_type(&[ptr_type.into()], false);
    assert!(codegen
        .check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure)
        .is_ok());
}

#[test]
fn test_musttail_abi_safety_small_struct_return_rejected() {
    let context = Context::create();
    let codegen = codegen_aarch64(&context, "test");
    let ptr_type = context.ptr_type(AddressSpace::default());
    // {ptr, ptr} = 16 bytes — even small structs are rejected on AArch64
    // because LLVM 18 musttail + indirect calls can crash in SelectionDAGISel
    let small_struct = context.struct_type(&[ptr_type.into(), ptr_type.into()], false);
    let fn_type = small_struct.fn_type(&[ptr_type.into()], false);
    let result = codegen.check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "struct return (musttail incompatible in LLVM 18)"
    );
}

#[test]
fn test_musttail_abi_safety_large_struct_return_skipped() {
    let context = Context::create();
    let codegen = codegen_aarch64(&context, "test");
    let i64_type = context.i64_type();
    let ptr_type = context.ptr_type(AddressSpace::default());
    // {i64, i64, i64} = 24 bytes — also rejected (all struct returns are)
    let large_struct =
        context.struct_type(&[i64_type.into(), i64_type.into(), i64_type.into()], false);
    let fn_type = large_struct.fn_type(&[ptr_type.into()], false);
    let result = codegen.check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "struct return (musttail incompatible in LLVM 18)"
    );
}

#[test]
fn test_musttail_abi_safety_struct_param_skipped() {
    let context = Context::create();
    let codegen = codegen_aarch64(&context, "test");
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let struct_param = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    let fn_type = ptr_type.fn_type(&[ptr_type.into(), struct_param.into()], false);
    let result = codegen.check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "struct parameter (musttail incompatible in LLVM 18)"
    );
}

// ========================================================================
// Tests for classify_musttail_target
// ========================================================================

#[test]
fn test_classify_musttail_target_x86_64() {
    assert_eq!(
        classify_musttail_target("x86_64-unknown-linux-gnu"),
        MusttailAbiTarget::X86_64
    );
}

#[test]
fn test_classify_musttail_target_amd64() {
    assert_eq!(
        classify_musttail_target("amd64-unknown-freebsd"),
        MusttailAbiTarget::X86_64
    );
}

#[test]
fn test_classify_musttail_target_aarch64() {
    assert_eq!(
        classify_musttail_target("aarch64-unknown-linux-gnu"),
        MusttailAbiTarget::AArch64
    );
}

#[test]
fn test_classify_musttail_target_arm64() {
    assert_eq!(
        classify_musttail_target("arm64-apple-darwin"),
        MusttailAbiTarget::AArch64
    );
}

#[test]
fn test_classify_musttail_target_unknown() {
    assert_eq!(
        classify_musttail_target("riscv64-unknown-linux-gnu"),
        MusttailAbiTarget::Unknown
    );
}

#[test]
fn test_classify_musttail_target_empty_string() {
    assert_eq!(classify_musttail_target(""), MusttailAbiTarget::Unknown);
}

// ========================================================================
// Architecture-conditional ABI guard tests (ADR 12.5.26e)
// ========================================================================

#[test]
fn test_musttail_abi_safety_x86_64_indirect_rejects_struct_return() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("x86_64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let struct_ret = context.struct_type(&[ptr_type.into(), ptr_type.into()], false);
    let fn_type = struct_ret.fn_type(&[ptr_type.into()], false);
    let result = codegen.check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure);
    assert!(
        result.is_err(),
        "indirect closure with struct return should be rejected even on x86_64"
    );
}

#[test]
fn test_musttail_abi_safety_x86_64_indirect_rejects_struct_param() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("x86_64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let struct_param = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    let fn_type = ptr_type.fn_type(&[ptr_type.into(), struct_param.into()], false);
    let result = codegen.check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure);
    assert!(
        result.is_err(),
        "indirect closure with struct param should be rejected even on x86_64"
    );
}

#[test]
fn test_musttail_abi_safety_x86_64_indirect_rejects_large_struct_return() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("x86_64-unknown-linux-gnu"));
    let i64_type = context.i64_type();
    let ptr_type = context.ptr_type(AddressSpace::default());
    let large_struct =
        context.struct_type(&[i64_type.into(), i64_type.into(), i64_type.into()], false);
    let fn_type = large_struct.fn_type(&[ptr_type.into()], false);
    let result = codegen.check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure);
    assert!(
        result.is_err(),
        "indirect closure with large struct return should be rejected even on x86_64"
    );
}

#[test]
fn test_musttail_abi_safety_unknown_target_rejects_struct_return() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("riscv64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let struct_ret = context.struct_type(&[ptr_type.into(), ptr_type.into()], false);
    let fn_type = struct_ret.fn_type(&[ptr_type.into()], false);
    let result = codegen.check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "struct return (musttail incompatible in LLVM 18)"
    );
}
