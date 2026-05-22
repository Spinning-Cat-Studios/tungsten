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
fn test_musttail_abi_safety_unknown_target_rejects_struct_param() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("riscv64-unknown-linux-gnu"));
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

#[test]
fn test_musttail_abi_safety_aarch64_explicit_rejects_struct_return() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
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

// ========================================================================
// Direct vs indirect call-kind ABI guard tests (ADR 12.5.26f)
// ========================================================================

#[test]
fn test_musttail_abi_safety_aarch64_direct_rejects_struct_return() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let struct_ret = context.struct_type(&[ptr_type.into(), ptr_type.into()], false);
    let fn_type = struct_ret.fn_type(&[ptr_type.into()], false);
    let result = codegen.check_musttail_abi_safety(fn_type, MusttailCallKind::DirectSelfRecursive);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "struct return (musttail incompatible in LLVM 18)"
    );
}

#[test]
fn test_musttail_abi_safety_aarch64_direct_rejects_struct_param() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let struct_param = context.struct_type(&[ptr_type.into(), i64_type.into()], false);
    let fn_type = ptr_type.fn_type(&[ptr_type.into(), struct_param.into()], false);
    let result = codegen.check_musttail_abi_safety(fn_type, MusttailCallKind::DirectSelfRecursive);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "struct parameter (musttail incompatible in LLVM 18)"
    );
}

#[test]
fn test_musttail_abi_safety_aarch64_indirect_rejects_struct_param() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
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

#[test]
fn test_musttail_abi_safety_unknown_direct_rejects_struct_return() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("riscv64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let struct_ret = context.struct_type(&[ptr_type.into(), ptr_type.into()], false);
    let fn_type = struct_ret.fn_type(&[ptr_type.into()], false);
    let result = codegen.check_musttail_abi_safety(fn_type, MusttailCallKind::DirectSelfRecursive);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "struct return (musttail incompatible in LLVM 18)"
    );
}

// ========================================================================
// Positive-path edge cases (non-struct types should always allow musttail)
// ========================================================================

#[test]
fn test_musttail_abi_safety_x86_64_direct_rejects_struct_return() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("x86_64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let struct_ret = context.struct_type(&[ptr_type.into(), ptr_type.into()], false);
    let fn_type = struct_ret.fn_type(&[ptr_type.into()], false);
    assert!(
        codegen
            .check_musttail_abi_safety(fn_type, MusttailCallKind::DirectSelfRecursive)
            .is_err(),
        "struct return should be rejected on all targets including x86_64"
    );
}

#[test]
fn test_musttail_abi_safety_aarch64_direct_allows_void_return() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_type = context
        .void_type()
        .fn_type(&[ptr_type.into(), ptr_type.into()], false);
    assert!(codegen
        .check_musttail_abi_safety(fn_type, MusttailCallKind::DirectSelfRecursive)
        .is_ok());
}

#[test]
fn test_musttail_abi_safety_aarch64_indirect_allows_ptr_return() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("aarch64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
    assert!(codegen
        .check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure)
        .is_ok());
}

#[test]
fn test_musttail_abi_safety_x86_64_direct_allows_ptr_return() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("x86_64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
    assert!(codegen
        .check_musttail_abi_safety(fn_type, MusttailCallKind::DirectSelfRecursive)
        .is_ok());
}

#[test]
fn test_musttail_abi_safety_x86_64_indirect_allows_void_return() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test");
    codegen
        .module
        .set_triple(&TargetTriple::create("x86_64-unknown-linux-gnu"));
    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_type = context
        .void_type()
        .fn_type(&[ptr_type.into(), ptr_type.into()], false);
    assert!(codegen
        .check_musttail_abi_safety(fn_type, MusttailCallKind::IndirectClosure)
        .is_ok());
}
