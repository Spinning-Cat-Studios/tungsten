//! Tests for `compute_musttail_blockers` — structured ABI blocker records
//! (ADR 1.7.26b §2.4). Complements the `&str`-gate tests in `tests.rs`.

use super::*;
use crate::codegen::musttail_report::{BlockerPosition, ReasonCode};
use inkwell::context::Context;
use inkwell::AddressSpace;

#[test]
fn no_blockers_for_scalar_signature() {
    let context = Context::create();
    let ptr = context.ptr_type(AddressSpace::default());
    let i64 = context.i64_type();
    // i64(ptr env, i64) — all scalar
    let fn_type = i64.fn_type(&[ptr.into(), i64.into()], false);
    let blockers = CodeGen::compute_musttail_blockers(fn_type);
    assert!(blockers.is_empty());
}

#[test]
fn struct_return_is_a_return_blocker() {
    let context = Context::create();
    let ptr = context.ptr_type(AddressSpace::default());
    let ret = context.struct_type(&[ptr.into(), ptr.into()], false);
    let fn_type = ret.fn_type(&[ptr.into()], false);
    let blockers = CodeGen::compute_musttail_blockers(fn_type);
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].position, BlockerPosition::Return);
    assert_eq!(blockers[0].reason, ReasonCode::StructReturn);
}

#[test]
fn flattenable_struct_param_is_struct_param() {
    let context = Context::create();
    let ptr = context.ptr_type(AddressSpace::default());
    let i64 = context.i64_type();
    // { ptr, i64 } is field-flattenable → STRUCT_PARAM (decomposable)
    let flat = context.struct_type(&[ptr.into(), i64.into()], false);
    let fn_type = i64.fn_type(&[ptr.into(), flat.into()], false);
    let blockers = CodeGen::compute_musttail_blockers(fn_type);
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].position, BlockerPosition::Param(1));
    assert_eq!(blockers[0].reason, ReasonCode::StructParam);
}

#[test]
fn nested_struct_param_is_non_flattenable() {
    let context = Context::create();
    let ptr = context.ptr_type(AddressSpace::default());
    let i64 = context.i64_type();
    let inner = context.struct_type(&[i64.into(), i64.into()], false);
    // { ptr, { i64, i64 } } has a nested struct field → NON_FLATTENABLE_PARAM
    let nested = context.struct_type(&[ptr.into(), inner.into()], false);
    let fn_type = i64.fn_type(&[ptr.into(), nested.into()], false);
    let blockers = CodeGen::compute_musttail_blockers(fn_type);
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].reason, ReasonCode::NonFlattenableParam);
}

#[test]
fn struct_param_and_return_yields_both_blockers_return_first() {
    let context = Context::create();
    let ptr = context.ptr_type(AddressSpace::default());
    let i64 = context.i64_type();
    let ret = context.struct_type(&[ptr.into(), ptr.into()], false);
    let param = context.struct_type(&[ptr.into(), i64.into()], false);
    // struct return AND struct param — the collect_type_names shape
    let fn_type = ret.fn_type(&[ptr.into(), param.into()], false);
    let blockers = CodeGen::compute_musttail_blockers(fn_type);
    assert_eq!(blockers.len(), 2);
    // Return is checked before params (matches the gate ordering).
    assert_eq!(blockers[0].position, BlockerPosition::Return);
    assert_eq!(blockers[0].reason, ReasonCode::StructReturn);
    assert_eq!(blockers[1].position, BlockerPosition::Param(1));
    assert_eq!(blockers[1].reason, ReasonCode::StructParam);
}

#[test]
fn env_ptr_param_zero_is_never_a_blocker() {
    let context = Context::create();
    let ptr = context.ptr_type(AddressSpace::default());
    // Only param is the env ptr (scalar) → no blockers.
    let fn_type = ptr.fn_type(&[ptr.into()], false);
    let blockers = CodeGen::compute_musttail_blockers(fn_type);
    assert!(blockers.is_empty());
}
