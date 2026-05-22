use super::*;

use crate::elaborate::env::definitions::{TypeDef, TypeDefKind};
use crate::span::Span;

fn make_env_with_record(
    env: &mut Env,
    type_name: &str,
    type_vis: Visibility,
    fields: Vec<(&str, Option<Visibility>)>,
    module: &ModulePath,
) {
    let dummy_span = Span::new(0, 0);
    let field_visibilities: Vec<Option<Visibility>> = fields.iter().map(|(_, vis)| *vis).collect();
    let record_fields: Vec<(String, tungsten_core::Type)> = fields
        .iter()
        .map(|(name, _)| (name.to_string(), tungsten_core::Type::Nat))
        .collect();
    let type_def = TypeDef {
        name: type_name.to_string(),
        params: vec![],
        kind: TypeDefKind::Record(record_fields),
        visibility: type_vis,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities,
    };
    env.define_type_in_module(type_def, module.clone());
}

#[test]
fn test_field_inherits_pub_from_parent() {
    let mut env = Env::new();
    let module = ModulePath::from_name("mod_a");
    make_env_with_record(
        &mut env,
        "Config",
        Visibility::Public,
        vec![("name", None), ("id", None)],
        &module,
    );
    assert_eq!(
        env.get_record_field_visibility("Config", 0),
        Some(Visibility::Public)
    );
    assert_eq!(
        env.get_record_field_visibility("Config", 1),
        Some(Visibility::Public)
    );
}

#[test]
fn test_field_explicit_visibility_overrides_parent() {
    let mut env = Env::new();
    let module = ModulePath::from_name("mod_a");
    make_env_with_record(
        &mut env,
        "Config",
        Visibility::Public,
        vec![
            ("name", Some(Visibility::Public)),
            ("internal_id", Some(Visibility::Crate)),
            ("secret", Some(Visibility::Private)),
        ],
        &module,
    );
    assert_eq!(
        env.get_record_field_visibility("Config", 0),
        Some(Visibility::Public)
    );
    assert_eq!(
        env.get_record_field_visibility("Config", 1),
        Some(Visibility::Crate)
    );
    assert_eq!(
        env.get_record_field_visibility("Config", 2),
        Some(Visibility::Private)
    );
}

#[test]
fn test_private_field_not_accessible_from_other_module() {
    let mut env = Env::new();
    let mod_a = ModulePath::from_name("mod_a");
    let mod_b = ModulePath::from_name("mod_b");
    env.register_module(mod_a.clone());
    env.register_module(mod_b.clone());
    make_env_with_record(
        &mut env,
        "Config",
        Visibility::Public,
        vec![
            ("name", Some(Visibility::Public)),
            ("secret", Some(Visibility::Private)),
        ],
        &mod_a,
    );
    // Public field accessible from mod_b
    assert!(env.is_record_field_accessible("Config", 0, &mod_b, true));
    // Private field not accessible from mod_b
    assert!(!env.is_record_field_accessible("Config", 1, &mod_b, true));
    // Private field accessible from mod_a
    assert!(env.is_record_field_accessible("Config", 1, &mod_a, true));
}

#[test]
fn test_crate_field_accessible_within_crate() {
    let mut env = Env::new();
    let mod_a = ModulePath::from_name("mod_a");
    let mod_b = ModulePath::from_name("mod_b");
    env.register_module(mod_a.clone());
    env.register_module(mod_b.clone());
    make_env_with_record(
        &mut env,
        "Config",
        Visibility::Public,
        vec![("internal_id", Some(Visibility::Crate))],
        &mod_a,
    );
    // Crate field accessible within crate
    assert!(env.is_record_field_accessible("Config", 0, &mod_b, true));
    // Not accessible from different crate
    assert!(!env.is_record_field_accessible("Config", 0, &mod_b, false));
}
