use super::*;

use crate::elaborate::env::definitions::{Constructor, TypeDef, TypeDefKind};
use crate::span::Span;

pub(super) fn make_env_with_type(
    env: &mut Env,
    type_name: &str,
    type_vis: Visibility,
    ctors: Vec<(&str, Option<Visibility>)>,
    module: &ModulePath,
) {
    let dummy_span = Span::new(0, 0);
    let constructors: Vec<Constructor> = ctors
        .iter()
        .enumerate()
        .map(|(i, (name, vis))| Constructor {
            name: name.to_string(),
            index: i,
            fields: vec![],
            visibility: *vis,
            span: dummy_span,
        })
        .collect();
    let type_def = TypeDef {
        name: type_name.to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(constructors),
        visibility: type_vis,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    };
    env.define_type_in_module(type_def, module.clone());
}

#[test]
fn test_constructor_inherits_pub_from_parent() {
    let mut env = Env::new();
    let module = ModulePath::from_name("mod_a");
    make_env_with_type(
        &mut env,
        "Token",
        Visibility::Public,
        vec![("Ident", None), ("Number", None)],
        &module,
    );
    let ctor = env.lookup_constructor("Ident").unwrap().clone();
    assert_eq!(
        env.get_constructor_visibility(&ctor),
        Some(Visibility::Public)
    );
}

#[test]
fn test_constructor_inherits_private_from_parent() {
    let mut env = Env::new();
    let module = ModulePath::from_name("mod_a");
    make_env_with_type(
        &mut env,
        "Secret",
        Visibility::Private,
        vec![("Hidden", None)],
        &module,
    );
    let ctor = env.lookup_constructor("Hidden").unwrap().clone();
    assert_eq!(
        env.get_constructor_visibility(&ctor),
        Some(Visibility::Private)
    );
}

#[test]
fn test_constructor_explicit_visibility_overrides_parent() {
    let mut env = Env::new();
    let module = ModulePath::from_name("mod_a");
    make_env_with_type(
        &mut env,
        "Token",
        Visibility::Public,
        vec![
            ("Ident", Some(Visibility::Public)),
            ("Number", Some(Visibility::Crate)),
            ("Internal", Some(Visibility::Private)),
        ],
        &module,
    );
    let ident = env.lookup_constructor("Ident").unwrap().clone();
    let number = env.lookup_constructor("Number").unwrap().clone();
    let internal = env.lookup_constructor("Internal").unwrap().clone();
    assert_eq!(
        env.get_constructor_visibility(&ident),
        Some(Visibility::Public)
    );
    assert_eq!(
        env.get_constructor_visibility(&number),
        Some(Visibility::Crate)
    );
    assert_eq!(
        env.get_constructor_visibility(&internal),
        Some(Visibility::Private)
    );
}

#[test]
fn test_private_constructor_not_accessible_from_other_module() {
    let mut env = Env::new();
    let mod_a = ModulePath::from_name("mod_a");
    let mod_b = ModulePath::from_name("mod_b");
    env.register_module(mod_a.clone());
    env.register_module(mod_b.clone());
    make_env_with_type(
        &mut env,
        "Token",
        Visibility::Public,
        vec![("Internal", Some(Visibility::Private))],
        &mod_a,
    );
    let ctor = env.lookup_constructor("Internal").unwrap().clone();
    // Not accessible from mod_b
    assert!(!env.is_constructor_accessible(&ctor, &mod_b, true));
    // Accessible from mod_a (defining module)
    assert!(env.is_constructor_accessible(&ctor, &mod_a, true));
}

#[test]
fn test_crate_constructor_accessible_within_crate() {
    let mut env = Env::new();
    let mod_a = ModulePath::from_name("mod_a");
    let mod_b = ModulePath::from_name("mod_b");
    env.register_module(mod_a.clone());
    env.register_module(mod_b.clone());
    make_env_with_type(
        &mut env,
        "Token",
        Visibility::Public,
        vec![("Number", Some(Visibility::Crate))],
        &mod_a,
    );
    let ctor = env.lookup_constructor("Number").unwrap().clone();
    // Accessible within the crate
    assert!(env.is_constructor_accessible(&ctor, &mod_b, true));
    // Not accessible from different crate
    assert!(!env.is_constructor_accessible(&ctor, &mod_b, false));
}
