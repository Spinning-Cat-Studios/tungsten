//! Primitive type layout tests: i32, i64, ptr, i1, void, float, double.

use std::collections::HashMap;

use crate::dump_abi::layout::compute_layout;

#[test]
fn layout_i32() {
    let types = HashMap::new();
    let layout = compute_layout("i32", &types).unwrap();
    assert_eq!(layout.total_size, 4);
    assert_eq!(layout.max_align, 4);
    assert_eq!(layout.padding, 0);
}

#[test]
fn layout_i64() {
    let types = HashMap::new();
    let layout = compute_layout("i64", &types).unwrap();
    assert_eq!(layout.total_size, 8);
    assert_eq!(layout.max_align, 8);
}

#[test]
fn layout_ptr() {
    let types = HashMap::new();
    let layout = compute_layout("ptr", &types).unwrap();
    assert_eq!(layout.total_size, 8);
    assert_eq!(layout.max_align, 8);
}

#[test]
fn layout_i1() {
    let types = HashMap::new();
    let layout = compute_layout("i1", &types).unwrap();
    assert_eq!(layout.total_size, 1);
    assert_eq!(layout.max_align, 1);
}

#[test]
fn layout_void() {
    let types = HashMap::new();
    let layout = compute_layout("void", &types).unwrap();
    assert_eq!(layout.total_size, 0);
}

#[test]
fn layout_float() {
    let types = HashMap::new();
    let layout = compute_layout("float", &types).unwrap();
    assert_eq!(layout.total_size, 4);
    assert_eq!(layout.max_align, 4);
}

#[test]
fn layout_double() {
    let types = HashMap::new();
    let layout = compute_layout("double", &types).unwrap();
    assert_eq!(layout.total_size, 8);
    assert_eq!(layout.max_align, 8);
}
