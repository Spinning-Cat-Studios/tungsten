//! Tests for the indirect-param predicate `should_pass_param_by_indirect` and
//! the R6 primitive `by_value_aggregate_is_musttail_illegal` (ADR 1.7.26e §2.2).
//!
//! The predicate is true **exactly** for lowered structs that are musttail-unsafe
//! by-value **and** non-flattenable (nested struct / array field / >8 fields).
//! Flattenable structs keep the 18.5.26a decomposition path (false); recursive
//! ADTs already lower to `ptr` (false); scalars are false.

use super::*;
use inkwell::context::Context;
use inkwell::AddressSpace;

#[test]
fn flat_small_struct_is_not_indirect() {
    // { ptr, i64 } — 2 scalar fields, flattenable → decomposition path, NOT indirect.
    let context = Context::create();
    let ptr = context.ptr_type(AddressSpace::default());
    let i64_ty = context.i64_type();
    let st = context.struct_type(&[ptr.into(), i64_ty.into()], false);
    assert!(!CodeGen::should_pass_param_by_indirect(st.into()));
}

#[test]
fn flat_exactly_8_fields_is_not_indirect() {
    // Exactly 8 scalar fields — the flattenable boundary → NOT indirect.
    let context = Context::create();
    let i64_ty = context.i64_type();
    let fields: Vec<_> = (0..8).map(|_| i64_ty.into()).collect();
    let st = context.struct_type(&fields, false);
    assert!(!CodeGen::should_pass_param_by_indirect(st.into()));
}

#[test]
fn nested_struct_param_is_indirect() {
    // { {i64}, i64 } — a nested struct field → non-flattenable → indirect.
    let context = Context::create();
    let i64_ty = context.i64_type();
    let inner = context.struct_type(&[i64_ty.into()], false);
    let outer = context.struct_type(&[inner.into(), i64_ty.into()], false);
    assert!(CodeGen::should_pass_param_by_indirect(outer.into()));
}

#[test]
fn array_field_param_is_indirect() {
    // { [4 x i64] } — an array field → non-flattenable → indirect.
    let context = Context::create();
    let i64_ty = context.i64_type();
    let arr = i64_ty.array_type(4);
    let st = context.struct_type(&[arr.into()], false);
    assert!(CodeGen::should_pass_param_by_indirect(st.into()));
}

#[test]
fn over_8_fields_param_is_indirect() {
    // 9 scalar fields — exceeds the flatten cap → non-flattenable → indirect.
    let context = Context::create();
    let i64_ty = context.i64_type();
    let fields: Vec<_> = (0..9).map(|_| i64_ty.into()).collect();
    let st = context.struct_type(&fields, false);
    assert!(CodeGen::should_pass_param_by_indirect(st.into()));
}

#[test]
fn recursive_adt_ptr_param_is_not_indirect() {
    // Recursive ADTs (List, Option, ...) already lower to `ptr`, not a struct.
    let context = Context::create();
    let ptr = context.ptr_type(AddressSpace::default());
    assert!(!CodeGen::should_pass_param_by_indirect(ptr.into()));
}

#[test]
fn scalar_param_is_not_indirect() {
    let context = Context::create();
    let i64_ty = context.i64_type();
    assert!(!CodeGen::should_pass_param_by_indirect(i64_ty.into()));
}

#[test]
fn by_value_aggregate_primitive_flags_structs_only() {
    // The R6 decoupled primitive: any by-value struct is musttail-illegal; a
    // scalar or ptr is not. (Flattenability is handled by the predicate on top.)
    let context = Context::create();
    let ptr = context.ptr_type(AddressSpace::default());
    let i64_ty = context.i64_type();
    let st = context.struct_type(&[i64_ty.into()], false);
    assert!(CodeGen::by_value_aggregate_is_musttail_illegal(st.into()));
    assert!(!CodeGen::by_value_aggregate_is_musttail_illegal(ptr.into()));
    assert!(!CodeGen::by_value_aggregate_is_musttail_illegal(
        i64_ty.into()
    ));
}
