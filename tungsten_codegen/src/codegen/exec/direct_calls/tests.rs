use super::*;
use crate::codegen::musttail_report::Decision;
use crate::codegen::CodeGen;
use inkwell::context::Context;
use inkwell::AddressSpace;

/// A mutual-tail shape: `f` tail-calls a *different* function `g`, whose param
/// is a nested (non-flattenable, Class-P) struct.
///
/// ADR 5.8.26a D4b. This branch used to be `trace_musttail` only, so an
/// `f→g→f` Class-P cycle produced no [`MusttailDecision`] at all — the
/// `tco-coverage --gate` then iterated a row set that could not contain it and
/// printed `✓`, which is indistinguishable from a real pass.
///
/// The assertion is on the RECORD, not on the gate's exit code, deliberately:
/// the gate is green either way (`build_coverage` keeps `SkipNonSelf` out of
/// the self-recursive inventory, and `run_gate` fires only on `Decision::Skip`),
/// so a test that checked the exit code would pass just as happily with the
/// recording deleted again.
#[test]
fn non_self_tail_call_records_a_skip_non_self_decision() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_non_self_tail_record");
    cg.module
        .set_triple(&inkwell::targets::TargetTriple::create(
            "aarch64-unknown-linux-gnu",
        ));

    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    // A non-flattenable param — the Class-P shape mutual-tail musttail would
    // have to handle, and the reason this edge is worth recording at all.
    let inner = context.struct_type(&[i64_type.into(), i64_type.into()], false);
    let nested = context.struct_type(&[ptr_type.into(), inner.into()], false);
    let callee_ty = i64_type.fn_type(&[ptr_type.into(), nested.into()], false);
    let callee = cg.module.add_function("g$direct", callee_ty, None);

    // We are compiling `f$direct`; the tail call targets `g$direct`.
    cg.direct_calls.current_entry = Some("f$direct".to_string());
    let arg_vals: Vec<inkwell::values::BasicValueEnum> =
        vec![ptr_type.const_null().into(), nested.const_zero().into()];
    let args_meta: Vec<inkwell::values::BasicMetadataValueEnum> =
        vec![ptr_type.const_null().into(), nested.const_zero().into()];
    let site = DirectCallSite {
        direct: "g$direct",
        direct_fn: callee,
        lookup_name: "g",
        arg_vals: &arg_vals,
        args_meta: &args_meta,
    };

    let emitted = cg
        .try_emit_saturated_musttail(&site, true)
        .expect("the non-self branch returns Ok");
    assert!(
        emitted.is_none(),
        "no musttail is emitted for a non-self tail call"
    );

    let rows = cg.musttail_decisions();
    assert_eq!(rows.len(), 1, "exactly one decision recorded: {rows:?}");
    assert_eq!(rows[0].decision, Decision::SkipNonSelf);
    assert_eq!(rows[0].decision.code(), "SKIP_NON_SELF");
    assert_eq!(
        rows[0].base_name(),
        "g",
        "the row is keyed by the CALLEE — it is a fact about the edge"
    );
    assert!(
        !rows[0].decision.is_self_recursive(),
        "must not be folded into the self-recursive inventory"
    );
    assert!(
        !rows[0].decision.is_constant_stack(),
        "a non-self tail edge does not achieve constant stack"
    );
    // The Class-P blocker is what a future call-graph join would classify on.
    assert!(
        rows[0]
            .reasons
            .contains(&crate::codegen::musttail_report::ReasonCode::NonFlattenableParam),
        "the nested-struct param is recorded as a Class-P blocker: {:?}",
        rows[0].reasons
    );
}

/// A self-recursive tail call must keep recording EMIT — the new branch sits
/// after the self-call arms, so this pins that it did not swallow them.
#[test]
fn a_self_recursive_tail_call_is_unaffected_by_the_non_self_branch() {
    let context = Context::create();
    let mut cg = CodeGen::new(&context, "test_self_still_emits");
    cg.module
        .set_triple(&inkwell::targets::TargetTriple::create(
            "aarch64-unknown-linux-gnu",
        ));

    let ptr_type = context.ptr_type(AddressSpace::default());
    let i64_type = context.i64_type();
    let fn_type = i64_type.fn_type(&[ptr_type.into(), i64_type.into()], false);
    let function = cg.module.add_function("f$direct", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    cg.builder.position_at_end(entry);
    cg.compilation.current_fn = Some(function);
    cg.direct_calls.current_entry = Some("f$direct".to_string());

    let arg_vals: Vec<inkwell::values::BasicValueEnum> =
        vec![ptr_type.const_null().into(), i64_type.const_zero().into()];
    let args_meta: Vec<inkwell::values::BasicMetadataValueEnum> =
        vec![ptr_type.const_null().into(), i64_type.const_zero().into()];
    let site = DirectCallSite {
        direct: "f$direct",
        direct_fn: function,
        lookup_name: "f",
        arg_vals: &arg_vals,
        args_meta: &args_meta,
    };

    cg.try_emit_saturated_musttail(&site, true)
        .expect("self-recursive tail call");
    let rows = cg.musttail_decisions();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].decision, Decision::Emit);
}

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
