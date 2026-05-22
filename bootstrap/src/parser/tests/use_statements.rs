//! Tests for `use` statement parsing.

use super::{parse_ok, unwrap_use_decl, unwrap_use_tree};
use crate::ast::*;

#[test]
fn test_use_simple() {
    let file = parse_ok("use foo::bar;");
    assert_eq!(file.items.len(), 1);
    let u = unwrap_use_decl(&file);
    assert_eq!(u.visibility, Visibility::Private);
    let UseTree::Path(path) = unwrap_use_tree(&file) else {
        panic!("Expected path use tree");
    };
    assert_eq!(path.segments.len(), 2);
    assert_eq!(path.segments[0].name, "foo");
    assert_eq!(path.segments[1].name, "bar");
}

#[test]
fn test_use_three_segments() {
    let file = parse_ok("use api::types::Config;");
    assert_eq!(file.items.len(), 1);
    let UseTree::Path(path) = unwrap_use_tree(&file) else {
        panic!("Expected path use tree");
    };
    assert_eq!(path.segments.len(), 3);
    assert_eq!(path.segments[0].name, "api");
    assert_eq!(path.segments[1].name, "types");
    assert_eq!(path.segments[2].name, "Config");
}

#[test]
fn test_use_grouped() {
    let file = parse_ok("use api::{Config, Error};");
    assert_eq!(file.items.len(), 1);
    let u = unwrap_use_decl(&file);
    assert_eq!(u.visibility, Visibility::Private);
    let UseTree::Group { prefix, items, .. } = unwrap_use_tree(&file) else {
        panic!("Expected group use tree");
    };
    assert_eq!(prefix.segments.len(), 1);
    assert_eq!(prefix.segments[0].name, "api");
    assert_eq!(items.len(), 2);
    let UseTree::Path(p0) = &items[0] else {
        panic!("Expected path")
    };
    assert_eq!(p0.segments[0].name, "Config");
    let UseTree::Path(p1) = &items[1] else {
        panic!("Expected path")
    };
    assert_eq!(p1.segments[0].name, "Error");
}

#[test]
fn test_use_grouped_trailing_comma() {
    let file = parse_ok("use api::{Config, Error,};");
    assert_eq!(file.items.len(), 1);
    let UseTree::Group { items, .. } = unwrap_use_tree(&file) else {
        panic!("Expected group use tree");
    };
    assert_eq!(items.len(), 2);
}

#[test]
fn test_pub_use() {
    let file = parse_ok("pub use internal::Helper;");
    assert_eq!(file.items.len(), 1);
    let u = unwrap_use_decl(&file);
    assert_eq!(u.visibility, Visibility::Public);
    let UseTree::Path(path) = unwrap_use_tree(&file) else {
        panic!("Expected path use tree");
    };
    assert_eq!(path.segments.len(), 2);
    assert_eq!(path.segments[0].name, "internal");
    assert_eq!(path.segments[1].name, "Helper");
}

#[test]
fn test_use_tree_expand() {
    let file = parse_ok("use api::types::{Config, Error};");
    let ExpandedUseTree::Paths(expanded) = unwrap_use_tree(&file).expand() else {
        panic!("Expected paths, got glob");
    };
    assert_eq!(expanded.len(), 2);
    assert_eq!(expanded[0].segments.len(), 3);
    assert_eq!(expanded[0].segments[0].name, "api");
    assert_eq!(expanded[0].segments[1].name, "types");
    assert_eq!(expanded[0].segments[2].name, "Config");
    assert_eq!(expanded[1].segments.len(), 3);
    assert_eq!(expanded[1].segments[0].name, "api");
    assert_eq!(expanded[1].segments[1].name, "types");
    assert_eq!(expanded[1].segments[2].name, "Error");
}

#[test]
fn test_use_glob() {
    let file = parse_ok("use foo::*;");
    assert_eq!(file.items.len(), 1);
    let UseTree::Glob { prefix, .. } = unwrap_use_tree(&file) else {
        panic!("Expected glob use tree");
    };
    assert_eq!(prefix.segments.len(), 1);
    assert_eq!(prefix.segments[0].name, "foo");
}

#[test]
fn test_use_glob_deep() {
    let file = parse_ok("use foo::bar::baz::*;");
    assert_eq!(file.items.len(), 1);
    let UseTree::Glob { prefix, .. } = unwrap_use_tree(&file) else {
        panic!("Expected glob use tree");
    };
    assert_eq!(prefix.segments.len(), 3);
    assert_eq!(prefix.segments[0].name, "foo");
    assert_eq!(prefix.segments[1].name, "bar");
    assert_eq!(prefix.segments[2].name, "baz");
}

#[test]
fn test_use_glob_expand() {
    let file = parse_ok("use api::types::*;");
    let ExpandedUseTree::Glob { prefix, .. } = unwrap_use_tree(&file).expand() else {
        panic!("Expected glob, got paths");
    };
    assert_eq!(prefix.segments.len(), 2);
    assert_eq!(prefix.segments[0].name, "api");
    assert_eq!(prefix.segments[1].name, "types");
}

#[test]
fn test_pub_use_glob() {
    let file = parse_ok("pub use internal::*;");
    assert_eq!(file.items.len(), 1);
    let u = unwrap_use_decl(&file);
    assert_eq!(u.visibility, Visibility::Public);
    let UseTree::Glob { prefix, .. } = unwrap_use_tree(&file) else {
        panic!("Expected glob use tree");
    };
    assert_eq!(prefix.segments.len(), 1);
    assert_eq!(prefix.segments[0].name, "internal");
}

// ── Import alias (as) tests ─────────────────────────────────────────

#[test]
fn test_use_alias_simple() {
    let file = parse_ok("use foo::Bar as Baz;");
    assert_eq!(file.items.len(), 1);
    let UseTree::Alias { path, alias, .. } = unwrap_use_tree(&file) else {
        panic!("Expected alias use tree");
    };
    assert_eq!(path.segments.len(), 2);
    assert_eq!(path.segments[0].name, "foo");
    assert_eq!(path.segments[1].name, "Bar");
    assert_eq!(alias.name, "Baz");
}

#[test]
fn test_use_alias_expand() {
    let file = parse_ok("use api::Config as Cfg;");
    let expanded = unwrap_use_tree(&file).expand();
    let ExpandedUseTree::Alias { path, alias, .. } = expanded else {
        panic!("Expected alias, got {:?}", expanded);
    };
    assert_eq!(path.segments.len(), 2);
    assert_eq!(path.segments[0].name, "api");
    assert_eq!(path.segments[1].name, "Config");
    assert_eq!(alias.name, "Cfg");
}

#[test]
fn test_use_grouped_alias_expand_all() {
    let file = parse_ok("use api::{Config as C, Error as E};");
    let all = unwrap_use_tree(&file).expand_all();
    assert_eq!(all.len(), 2);

    let ExpandedUseTree::Alias {
        path: p0,
        alias: a0,
        ..
    } = &all[0]
    else {
        panic!("Expected alias for first item");
    };
    assert_eq!(p0.segments.last().unwrap().name, "Config");
    assert_eq!(a0.name, "C");

    let ExpandedUseTree::Alias {
        path: p1,
        alias: a1,
        ..
    } = &all[1]
    else {
        panic!("Expected alias for second item");
    };
    assert_eq!(p1.segments.last().unwrap().name, "Error");
    assert_eq!(a1.name, "E");
}
