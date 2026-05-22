use super::*;

#[test]
fn parse_base_types() {
    assert_eq!(parse_type("Nat").unwrap(), TypeAst::Base("Nat".into()));
    assert_eq!(parse_type("Bool").unwrap(), TypeAst::Base("Bool".into()));
    assert_eq!(parse_type("Unit").unwrap(), TypeAst::Base("Unit".into()));
    assert_eq!(parse_type("Void").unwrap(), TypeAst::Base("Void".into()));
    assert_eq!(
        parse_type("String").unwrap(),
        TypeAst::Base("String".into())
    );
    assert_eq!(parse_type("Prop").unwrap(), TypeAst::Base("Prop".into()));
}

#[test]
fn parse_type_variable() {
    assert_eq!(parse_type("T").unwrap(), TypeAst::TyVar("T".into()));
    assert_eq!(
        parse_type("α_List").unwrap(),
        TypeAst::TyVar("α_List".into())
    );
}

#[test]
fn parse_arrow() {
    assert_eq!(
        parse_type("(Nat → Bool)").unwrap(),
        TypeAst::Arrow(
            Box::new(TypeAst::Base("Nat".into())),
            Box::new(TypeAst::Base("Bool".into()))
        )
    );
}

#[test]
fn parse_product() {
    assert_eq!(
        parse_type("(Nat × Bool)").unwrap(),
        TypeAst::Product(
            Box::new(TypeAst::Base("Nat".into())),
            Box::new(TypeAst::Base("Bool".into()))
        )
    );
}

#[test]
fn parse_sum() {
    assert_eq!(
        parse_type("(Unit + Nat)").unwrap(),
        TypeAst::Sum(
            Box::new(TypeAst::Base("Unit".into())),
            Box::new(TypeAst::Base("Nat".into()))
        )
    );
}

#[test]
fn parse_mu() {
    let ast = parse_type("μα_List. (Unit + (Nat × α_List))").unwrap();
    assert_eq!(
        ast,
        TypeAst::Mu(
            "α_List".into(),
            Box::new(TypeAst::Sum(
                Box::new(TypeAst::Base("Unit".into())),
                Box::new(TypeAst::Product(
                    Box::new(TypeAst::Base("Nat".into())),
                    Box::new(TypeAst::TyVar("α_List".into()))
                ))
            ))
        )
    );
}

#[test]
fn parse_forall() {
    let ast = parse_type("∀T. (T → T)").unwrap();
    assert_eq!(
        ast,
        TypeAst::Forall(
            "T".into(),
            Box::new(TypeAst::Arrow(
                Box::new(TypeAst::TyVar("T".into())),
                Box::new(TypeAst::TyVar("T".into()))
            ))
        )
    );
}

#[test]
fn parse_error_type() {
    assert_eq!(parse_type("<type error>").unwrap(), TypeAst::Error);
}

#[test]
fn parse_empty_is_error() {
    assert!(parse_type("").is_err());
}

#[test]
fn parse_malformed_is_error() {
    assert!(parse_type("(Nat →").is_err());
    assert!(parse_type("→ Nat").is_err());
    assert!(parse_type("μ").is_err());
}
