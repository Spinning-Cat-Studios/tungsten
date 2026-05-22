//! Layout computation tests: structs, arrays, ADTs, named types, error cases.

use std::collections::HashMap;

use crate::dump_abi::layout::{compute_layout, LayoutError};

// ─── Struct layout tests ─────────────────────────────────────────────

#[test]
fn layout_simple_struct() {
    let types = HashMap::new();
    // { i32, i64 } → size=16 (4 + 4pad + 8), align=8
    let layout = compute_layout("{ i32, i64 }", &types).unwrap();
    assert_eq!(layout.total_size, 16);
    assert_eq!(layout.max_align, 8);
    assert_eq!(layout.padding, 4); // 4 bytes padding between i32 and i64
    assert_eq!(layout.fields.len(), 2);
    assert_eq!(layout.fields[0].offset, 0);
    assert_eq!(layout.fields[0].size, 4);
    assert_eq!(layout.fields[1].offset, 8);
    assert_eq!(layout.fields[1].size, 8);
}

#[test]
fn layout_no_padding_struct() {
    let types = HashMap::new();
    // { i32, i32 } → size=8, align=4, no padding
    let layout = compute_layout("{ i32, i32 }", &types).unwrap();
    assert_eq!(layout.total_size, 8);
    assert_eq!(layout.max_align, 4);
    assert_eq!(layout.padding, 0);
}

#[test]
fn layout_empty_struct() {
    let types = HashMap::new();
    let layout = compute_layout("{}", &types).unwrap();
    assert_eq!(layout.total_size, 0);
    assert_eq!(layout.max_align, 1);
    assert_eq!(layout.padding, 0);
    assert!(layout.fields.is_empty());
}

#[test]
fn layout_nested_struct() {
    let types = HashMap::new();
    // { i32, { i64, i32 } }
    // Inner: { i64, i32 } → offset=0:i64(8), offset=8:i32(4) → size=16, align=8, pad=4
    // Outer: offset=0:i32(4), offset=8:inner(16) → size=24, align=8, pad=4
    let layout = compute_layout("{ i32, { i64, i32 } }", &types).unwrap();
    assert_eq!(layout.total_size, 24);
    assert_eq!(layout.max_align, 8);
    assert_eq!(layout.fields.len(), 2);
    assert_eq!(layout.fields[0].offset, 0); // i32
    assert_eq!(layout.fields[1].offset, 8); // inner struct at align 8
    assert_eq!(layout.fields[1].size, 16);
}

// ─── Array layout tests ──────────────────────────────────────────────

#[test]
fn layout_array() {
    let types = HashMap::new();
    // [4 x i32] → size=16, align=4
    let layout = compute_layout("[4 x i32]", &types).unwrap();
    assert_eq!(layout.total_size, 16);
    assert_eq!(layout.max_align, 4);
}

#[test]
fn layout_zero_length_array() {
    let types = HashMap::new();
    // [0 x i8] → size=0, align=1
    let layout = compute_layout("[0 x i8]", &types).unwrap();
    assert_eq!(layout.total_size, 0);
    assert_eq!(layout.max_align, 1);
}

#[test]
fn layout_byte_array() {
    let types = HashMap::new();
    // [88 x i8] → size=88, align=1
    let layout = compute_layout("[88 x i8]", &types).unwrap();
    assert_eq!(layout.total_size, 88);
    assert_eq!(layout.max_align, 1);
}

// ─── ADT layout tests (tungsten-specific patterns) ───────────────────

#[test]
fn layout_ordering_adt() {
    let types = HashMap::new();
    // 3+-ctor ADT: { i32, [0 x i8] } → size=4, align=4
    let layout = compute_layout("{ i32, [0 x i8] }", &types).unwrap();
    assert_eq!(layout.total_size, 4);
    assert_eq!(layout.max_align, 4);
    assert_eq!(layout.fields.len(), 2);
    assert_eq!(layout.fields[0].size, 4); // tag
    assert_eq!(layout.fields[1].size, 0); // empty payload
}

#[test]
fn layout_item_adt() {
    let types = HashMap::new();
    // 3+-ctor ADT: { i32, [88 x i8] } → size=92, align=4
    let layout = compute_layout("{ i32, [88 x i8] }", &types).unwrap();
    assert_eq!(layout.total_size, 92);
    assert_eq!(layout.max_align, 4);
    assert_eq!(layout.fields.len(), 2);
    assert_eq!(layout.fields[0].offset, 0);
    assert_eq!(layout.fields[0].size, 4); // tag
    assert_eq!(layout.fields[1].offset, 4);
    assert_eq!(layout.fields[1].size, 88); // opaque payload
}

// ─── Named type resolution tests ─────────────────────────────────────

#[test]
fn layout_named_type_resolution() {
    let mut types = HashMap::new();
    types.insert("Ordering".to_string(), "{ i32, [0 x i8] }".to_string());

    let layout = compute_layout("%Ordering", &types).unwrap();
    assert_eq!(layout.total_size, 4);
    assert_eq!(layout.max_align, 4);
}

#[test]
fn layout_nested_named_types() {
    let mut types = HashMap::new();
    types.insert("Inner".to_string(), "{ i32, i64 }".to_string());
    types.insert("Outer".to_string(), "{ %Inner, i32 }".to_string());

    let layout = compute_layout("%Outer", &types).unwrap();
    // Inner: 16 bytes (4+4pad+8), align 8
    // Outer: offset=0:Inner(16), offset=16:i32(4) → 24 (round to align 8)
    assert_eq!(layout.total_size, 24);
    assert_eq!(layout.max_align, 8);
}

#[test]
fn layout_unresolved_type_error() {
    let types = HashMap::new();
    let result = compute_layout("%Unknown", &types);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, LayoutError::UnresolvedType(ref name) if name == "Unknown"),
        "expected UnresolvedType, got: {err}"
    );
}

#[test]
fn layout_recursive_through_ptr_is_safe() {
    let mut types = HashMap::new();
    // List = { i32, ptr } — recursion goes through ptr, which has fixed size
    types.insert("List".to_string(), "{ i32, ptr }".to_string());

    let layout = compute_layout("%List", &types).unwrap();
    // { i32, ptr } → size=16 (4+4pad+8), align=8
    assert_eq!(layout.total_size, 16);
    assert_eq!(layout.max_align, 8);
}

#[test]
fn layout_direct_cycle_rejected() {
    let mut types = HashMap::new();
    types.insert("A".to_string(), "{ i32, %B }".to_string());
    types.insert("B".to_string(), "{ i64, %A }".to_string());

    let result = compute_layout("%A", &types);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, LayoutError::CycleDetected(_)),
        "expected CycleDetected, got: {err}"
    );
}

// ─── Unsupported type tests ──────────────────────────────────────────

#[test]
fn layout_vector_unsupported() {
    let types = HashMap::new();
    let result = compute_layout("<4 x i32>", &types);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        LayoutError::UnsupportedType(_)
    ));
}

#[test]
fn layout_packed_struct_unsupported() {
    let types = HashMap::new();
    let result = compute_layout("<{ i32, i64 }>", &types);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        LayoutError::UnsupportedType(_)
    ));
}

#[test]
fn layout_i128_unsupported() {
    let types = HashMap::new();
    let result = compute_layout("i128", &types);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        LayoutError::UnsupportedType(_)
    ));
}
