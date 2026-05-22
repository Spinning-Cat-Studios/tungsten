//! Signature parsing tests: simple, struct, named type, and malformed inputs.

use crate::dump_abi::parse::parse_signature;

#[test]
fn parse_simple_signature() {
    let (ret, params) = parse_signature("define i64 @test(i32 %a, i64 %b)").unwrap();
    assert_eq!(ret, "i64");
    assert_eq!(params, vec!["i32", "i64"]);
}

#[test]
fn parse_void_return() {
    let (ret, params) = parse_signature("define void @init()").unwrap();
    assert_eq!(ret, "void");
    assert!(params.is_empty());
}

#[test]
fn parse_struct_param() {
    let (ret, params) =
        parse_signature("define i64 @compare({ i32, [0 x i8] } %a, { i32, [0 x i8] } %b)").unwrap();
    assert_eq!(ret, "i64");
    assert_eq!(params.len(), 2);
    assert_eq!(params[0], "{ i32, [0 x i8] }");
    assert_eq!(params[1], "{ i32, [0 x i8] }");
}

#[test]
fn parse_named_type_param() {
    let (ret, params) = parse_signature("define i64 @f(%Ordering %a)").unwrap();
    assert_eq!(ret, "i64");
    assert_eq!(params, vec!["%Ordering"]);
}

#[test]
fn parse_linkage_keywords() {
    let (ret, params) =
        parse_signature("define dso_local i64 @main(i32 %argc, ptr %argv)").unwrap();
    assert_eq!(ret, "i64");
    assert_eq!(params, vec!["i32", "ptr"]);
}

// ─── Malformed input tests ───────────────────────────────────────────

#[test]
fn parse_malformed_signature_no_at() {
    let result = parse_signature("define i64 test()");
    assert!(result.is_err());
}

#[test]
fn parse_malformed_no_parens() {
    let result = parse_signature("define i64 @test");
    assert!(result.is_err());
}
