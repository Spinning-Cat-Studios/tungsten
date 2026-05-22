//! Unit tests for info helper functions.

use crate::info::helpers::*;
use tungsten_bootstrap::elaborate::{AdtOrigin, TypeProvenance};
use tungsten_core::types::Type;

#[test]
fn test_format_type_short_simple() {
    assert_eq!(format_type_short(&Type::Nat), "Nat");
    assert_eq!(format_type_short(&Type::Bool), "Bool");
    assert_eq!(format_type_short(&Type::Unit), "Unit");
    assert_eq!(format_type_short(&Type::String), "String");
}

#[test]
fn test_format_type_short_truncation() {
    let long_type = Type::Arrow(
        Box::new(Type::Arrow(
            Box::new(Type::Arrow(
                Box::new(Type::Nat),
                Box::new(Type::Arrow(Box::new(Type::Bool), Box::new(Type::String))),
            )),
            Box::new(Type::Nat),
        )),
        Box::new(Type::Arrow(
            Box::new(Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool))),
            Box::new(Type::String),
        )),
    );
    let result = format_type_short(&long_type);
    assert!(
        result.len() <= 63,
        "Should be truncated, got {} chars: {}",
        result.len(),
        result
    );
    if result.contains("...") {
        assert!(result.ends_with("..."));
    }
}

#[test]
fn test_format_semantic_type_no_provenance() {
    let provenance = TypeProvenance::default();
    assert_eq!(format_semantic_type(&Type::Nat, &provenance), None);
    assert_eq!(
        format_semantic_type(
            &Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool)),
            &provenance
        ),
        None
    );
}

#[test]
fn test_format_semantic_type_with_provenance() {
    let mut provenance = TypeProvenance::default();
    provenance.mu_origins.insert(
        "α_List".to_string(),
        AdtOrigin {
            adt_name: "List".to_string(),
            type_args: vec![Type::String],
            constructors: vec!["Nil".to_string(), "Cons".to_string()],
        },
    );

    let mu_type = Type::Mu(
        "α_List".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::Product(
                Box::new(Type::String),
                Box::new(Type::TyVar("α_List".to_string())),
            )),
        )),
    );
    let result = format_semantic_type(&mu_type, &provenance);
    assert_eq!(result, Some("List<String>".to_string()));
}

#[test]
fn test_format_semantic_type_arrow_with_provenance() {
    let mut provenance = TypeProvenance::default();
    provenance.mu_origins.insert(
        "α_List".to_string(),
        AdtOrigin {
            adt_name: "List".to_string(),
            type_args: vec![Type::Nat],
            constructors: vec!["Nil".to_string(), "Cons".to_string()],
        },
    );

    let mu_list = Type::Mu(
        "α_List".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::Product(
                Box::new(Type::Nat),
                Box::new(Type::TyVar("α_List".to_string())),
            )),
        )),
    );

    let arrow = Type::Arrow(Box::new(mu_list), Box::new(Type::Nat));
    let result = format_semantic_type(&arrow, &provenance);
    assert_eq!(result, Some("List<Nat> -> Nat".to_string()));
}

#[test]
fn test_format_semantic_type_no_type_args() {
    let mut provenance = TypeProvenance::default();
    provenance.mu_origins.insert(
        "α_Token".to_string(),
        AdtOrigin {
            adt_name: "Token".to_string(),
            type_args: vec![],
            constructors: vec!["Ident".to_string(), "Number".to_string()],
        },
    );

    let mu_type = Type::Mu("α_Token".to_string(), Box::new(Type::Unit));
    let result = format_semantic_type(&mu_type, &provenance);
    assert_eq!(result, Some("Token".to_string()));
}

#[test]
fn test_encode_ctor_fields_empty() {
    let ctor = tungsten_bootstrap::elaborate::Constructor {
        name: "Nil".to_string(),
        fields: vec![],
        index: 0,
        visibility: None,
        span: tungsten_bootstrap::span::Span::new(0, 0),
    };
    assert_eq!(encode_ctor_fields(&ctor), "Unit");
}

#[test]
fn test_encode_ctor_fields_single() {
    let ctor = tungsten_bootstrap::elaborate::Constructor {
        name: "Some".to_string(),
        fields: vec![Type::Nat],
        index: 0,
        visibility: None,
        span: tungsten_bootstrap::span::Span::new(0, 0),
    };
    assert_eq!(encode_ctor_fields(&ctor), "Nat");
}

#[test]
fn test_encode_ctor_fields_multiple() {
    let ctor = tungsten_bootstrap::elaborate::Constructor {
        name: "Cons".to_string(),
        fields: vec![Type::Nat, Type::String],
        index: 1,
        visibility: None,
        span: tungsten_bootstrap::span::Span::new(0, 0),
    };
    assert_eq!(encode_ctor_fields(&ctor), "Nat × String");
}

#[test]
fn test_encode_body_description_empty() {
    assert_eq!(encode_body_description(&[], "α"), "Void");
}

#[test]
fn test_encode_body_description_two_ctors() {
    let ctors = vec![
        tungsten_bootstrap::elaborate::Constructor {
            name: "Nil".to_string(),
            fields: vec![],
            index: 0,
            visibility: None,
            span: tungsten_bootstrap::span::Span::new(0, 0),
        },
        tungsten_bootstrap::elaborate::Constructor {
            name: "Cons".to_string(),
            fields: vec![Type::Nat, Type::String],
            index: 1,
            visibility: None,
            span: tungsten_bootstrap::span::Span::new(0, 0),
        },
    ];
    assert_eq!(
        encode_body_description(&ctors, "α_List"),
        "Unit + Nat × String"
    );
}
