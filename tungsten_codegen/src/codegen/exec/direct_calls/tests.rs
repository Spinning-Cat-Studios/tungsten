use super::*;
use crate::codegen::CodeGen;
use inkwell::context::Context;
use inkwell::AddressSpace;

#[test]
fn test_direct_musttail_small_struct_return_accepted() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test_direct_small_struct");
    // Explicitly set AArch64 — this test verifies AArch64-specific struct return rejection
    codegen
        .module
        .set_triple(&inkwell::targets::TargetTriple::create(
            "aarch64-unknown-linux-gnu",
        ));

    // Create a function with {ptr, ptr} return (16 bytes).
    // LLVM 18 on AArch64 rejects musttail with struct returns even for
    // direct calls — "failed to perform tail call elimination".
    let ptr_type = context.ptr_type(AddressSpace::default());
    let small_struct = context.struct_type(&[ptr_type.into(), ptr_type.into()], false);
    let fn_type = small_struct.fn_type(&[ptr_type.into(), ptr_type.into(), ptr_type.into()], false);
    let function = codegen
        .module
        .add_function("small_ret$direct", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);

    let args: Vec<inkwell::values::BasicMetadataValueEnum> = vec![
        ptr_type.const_null().into(),
        ptr_type.const_null().into(),
        ptr_type.const_null().into(),
    ];

    // musttail should be SKIPPED — struct return incompatible on AArch64
    let result = codegen.try_emit_direct_musttail(function, &args);
    assert!(result.is_ok());
    assert!(
        result.unwrap().is_none(),
        "struct return should skip musttail on AArch64"
    );
}

#[test]
fn test_direct_musttail_large_struct_return_skipped() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "test_direct_large_struct");
    // Explicitly set AArch64 — this test verifies AArch64-specific struct return rejection
    codegen
        .module
        .set_triple(&inkwell::targets::TargetTriple::create(
            "aarch64-unknown-linux-gnu",
        ));

    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    // {i64, i64, i64} = 24 bytes — LLVM 18 rejects musttail with struct
    // returns on AArch64 for all call kinds.
    let large_struct =
        context.struct_type(&[i64_type.into(), i64_type.into(), i64_type.into()], false);
    let fn_type = large_struct.fn_type(&[ptr_type.into(), ptr_type.into()], false);
    let function = codegen
        .module
        .add_function("large_ret$direct", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);

    let args: Vec<inkwell::values::BasicMetadataValueEnum> =
        vec![ptr_type.const_null().into(), ptr_type.const_null().into()];

    // musttail should be SKIPPED — struct return incompatible on AArch64
    let result = codegen.try_emit_direct_musttail(function, &args);
    assert!(result.is_ok());
    assert!(
        result.unwrap().is_none(),
        "struct return should skip musttail on AArch64"
    );
}
