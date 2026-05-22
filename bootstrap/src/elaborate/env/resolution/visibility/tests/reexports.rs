use super::*;

use crate::elaborate::env::imports::ImportRequest;
use crate::span::Span;

#[test]
fn test_reexport_pub_use_of_pub_crate_stays_crate() {
    // Scenario: module `internal` has pub(crate) value, module `api` does `pub use`
    // Effective visibility should be Crate, not Public
    let mut env = Env::new();
    let internal = ModulePath::from_name("internal");
    let api = ModulePath::from_name("api");
    let consumer = ModulePath::from_name("consumer");
    env.register_module(internal.clone());
    env.register_module(api.clone());
    env.register_module(consumer.clone());

    // Register pub(crate) value in `internal`
    let dummy_span = Span::new(0, 0);
    env.define_value_in_module(
        crate::elaborate::env::definitions::ValueDef {
            name: "secret".to_string(),
            ty: tungsten_core::Type::Nat,
            visibility: Visibility::Crate,
            span: dummy_span,
        },
        internal.clone(),
    );

    // Module `api` re-exports with `pub use` (reexport_visibility = Public)
    env.add_value_import(
        &api,
        ImportRequest {
            local_name: "secret".to_string(),
            source_module: internal.clone(),
            original_name: "secret".to_string(),
            span: dummy_span,
            is_reexport: true,
            reexport_visibility: Some(Visibility::Public),
        },
    );

    // Module `consumer` imports from `api` (inherits the reexport_visibility)
    env.add_value_import(
        &consumer,
        ImportRequest {
            local_name: "secret".to_string(),
            source_module: api.clone(),
            original_name: "secret".to_string(),
            span: dummy_span,
            is_reexport: false,
            reexport_visibility: Some(Visibility::Public),
        },
    );

    // Effective visibility should be min(Crate, Public) = Crate
    let effective = env.effective_value_visibility("secret", Visibility::Crate, &consumer);
    assert_eq!(effective, Visibility::Crate);

    // Within the same crate, Crate visibility is accessible
    assert!(env.is_item_accessible(effective, &internal, &consumer, true));
    // From a different crate, Crate visibility is NOT accessible
    assert!(!env.is_item_accessible(effective, &internal, &consumer, false));
}

#[test]
fn test_reexport_pub_crate_use_of_pub_caps_to_crate() {
    // Scenario: module `lib` has pub value, module `api` does `pub(crate) use`
    // Effective visibility through the re-export should be Crate
    let mut env = Env::new();
    let lib = ModulePath::from_name("lib");
    let api = ModulePath::from_name("api");
    let consumer = ModulePath::from_name("consumer");
    env.register_module(lib.clone());
    env.register_module(api.clone());
    env.register_module(consumer.clone());

    let dummy_span = Span::new(0, 0);
    env.define_value_in_module(
        crate::elaborate::env::definitions::ValueDef {
            name: "public_fn".to_string(),
            ty: tungsten_core::Type::Nat,
            visibility: Visibility::Public,
            span: dummy_span,
        },
        lib.clone(),
    );

    // `api` does `pub(crate) use lib::public_fn`
    env.add_value_import(
        &consumer,
        ImportRequest {
            local_name: "public_fn".to_string(),
            source_module: api.clone(),
            original_name: "public_fn".to_string(),
            span: dummy_span,
            is_reexport: false,
            reexport_visibility: Some(Visibility::Crate),
        },
    );

    // Effective = min(Public, Crate) = Crate
    let effective = env.effective_value_visibility("public_fn", Visibility::Public, &consumer);
    assert_eq!(effective, Visibility::Crate);

    // From different crate, NOT accessible
    assert!(!env.is_item_accessible(effective, &lib, &consumer, false));
}

#[test]
fn test_reexport_pub_use_of_pub_stays_pub() {
    // pub use of a pub item → remains Public
    let mut env = Env::new();
    let lib = ModulePath::from_name("lib");
    let consumer = ModulePath::from_name("consumer");
    env.register_module(lib.clone());
    env.register_module(consumer.clone());

    let dummy_span = Span::new(0, 0);
    env.add_value_import(
        &consumer,
        ImportRequest {
            local_name: "public_fn".to_string(),
            source_module: lib.clone(),
            original_name: "public_fn".to_string(),
            span: dummy_span,
            is_reexport: false,
            reexport_visibility: Some(Visibility::Public),
        },
    );

    let effective = env.effective_value_visibility("public_fn", Visibility::Public, &consumer);
    assert_eq!(effective, Visibility::Public);
    // Note: is_item_accessible(Public, ..., false) returns false today because
    // cross-crate access isn't implemented yet. The key assertion is that
    // the effective visibility remains Public (not downgraded).
}

#[test]
fn test_reexport_no_reexport_visibility_uses_declared() {
    // Private `use` (no reexport_visibility) → declared visibility unchanged
    let mut env = Env::new();
    let lib = ModulePath::from_name("lib");
    let consumer = ModulePath::from_name("consumer");
    env.register_module(lib.clone());
    env.register_module(consumer.clone());

    let dummy_span = Span::new(0, 0);
    env.add_value_import(
        &consumer,
        ImportRequest {
            local_name: "helper".to_string(),
            source_module: lib.clone(),
            original_name: "helper".to_string(),
            span: dummy_span,
            is_reexport: false,
            reexport_visibility: None,
        },
    );

    let effective = env.effective_value_visibility("helper", Visibility::Public, &consumer);
    assert_eq!(effective, Visibility::Public);
}

#[test]
fn test_reexport_type_visibility_capped() {
    // Type re-export capping works the same way
    let mut env = Env::new();
    let internal = ModulePath::from_name("internal");
    let consumer = ModulePath::from_name("consumer");
    env.register_module(internal.clone());
    env.register_module(consumer.clone());

    let dummy_span = Span::new(0, 0);
    env.add_type_import(
        &consumer,
        ImportRequest {
            local_name: "Secret".to_string(),
            source_module: internal.clone(),
            original_name: "Secret".to_string(),
            span: dummy_span,
            is_reexport: false,
            reexport_visibility: Some(Visibility::Public),
        },
    );

    // Type declared as Crate, re-exported as Public → effective = Crate
    let effective = env.effective_type_visibility("Secret", Visibility::Crate, &consumer);
    assert_eq!(effective, Visibility::Crate);
}

#[test]
fn test_reexport_constructor_visibility_capped() {
    // Constructor re-export capping works the same way
    let mut env = Env::new();
    let internal = ModulePath::from_name("internal");
    let consumer = ModulePath::from_name("consumer");
    env.register_module(internal.clone());
    env.register_module(consumer.clone());

    let dummy_span = Span::new(0, 0);
    env.add_constructor_import(
        &consumer,
        ImportRequest {
            local_name: "Hidden".to_string(),
            source_module: internal.clone(),
            original_name: "Hidden".to_string(),
            span: dummy_span,
            is_reexport: false,
            reexport_visibility: Some(Visibility::Public),
        },
    );

    // Constructor declared as Crate, re-exported as Public → effective = Crate
    let effective = env.effective_constructor_visibility("Hidden", Visibility::Crate, &consumer);
    assert_eq!(effective, Visibility::Crate);
}

#[test]
fn test_reexport_private_outside_family_forbidden() {
    // Re-exporting a private item outside its module family should be blocked.
    // The effective visibility stays Private, so any non-family consumer is denied.
    let mut env = Env::new();
    let internal = ModulePath::from_name("internal");
    let api = ModulePath::from_name("api");
    let consumer = ModulePath::from_name("consumer");
    env.register_module(internal.clone());
    env.register_module(api.clone());
    env.register_module(consumer.clone());

    let dummy_span = Span::new(0, 0);
    env.define_value_in_module(
        crate::elaborate::env::definitions::ValueDef {
            name: "secret".to_string(),
            ty: tungsten_core::Type::Nat,
            visibility: Visibility::Private,
            span: dummy_span,
        },
        internal.clone(),
    );

    // `api` does `pub use internal::secret` — re-export is Public but item is Private
    env.add_value_import(
        &consumer,
        ImportRequest {
            local_name: "secret".to_string(),
            source_module: api.clone(),
            original_name: "secret".to_string(),
            span: dummy_span,
            is_reexport: false,
            reexport_visibility: Some(Visibility::Public),
        },
    );

    // Effective = min(Private, Public) = Private
    let effective = env.effective_value_visibility("secret", Visibility::Private, &consumer);
    assert_eq!(effective, Visibility::Private);

    // Private item NOT accessible from consumer (not in internal's module family)
    assert!(!env.is_item_accessible(effective, &internal, &consumer, true));
    // Also not accessible cross-crate
    assert!(!env.is_item_accessible(effective, &internal, &consumer, false));
}

#[test]
fn test_effective_visibility_chained_reexports() {
    // Verify capping composes through A → B → C re-export chain.
    // A has pub(crate) item, B does `pub use`, C imports from B.
    // Each hop should independently cap visibility.
    let mut env = Env::new();
    let mod_a = ModulePath::from_name("mod_a");
    let mod_b = ModulePath::from_name("mod_b");
    let mod_c = ModulePath::from_name("mod_c");
    env.register_module(mod_a.clone());
    env.register_module(mod_b.clone());
    env.register_module(mod_c.clone());

    let dummy_span = Span::new(0, 0);
    env.define_value_in_module(
        crate::elaborate::env::definitions::ValueDef {
            name: "helper".to_string(),
            ty: tungsten_core::Type::Nat,
            visibility: Visibility::Crate,
            span: dummy_span,
        },
        mod_a.clone(),
    );

    // B does `pub use A::helper` (reexport_visibility = Public)
    env.add_value_import(
        &mod_b,
        ImportRequest {
            local_name: "helper".to_string(),
            source_module: mod_a.clone(),
            original_name: "helper".to_string(),
            span: dummy_span,
            is_reexport: true,
            reexport_visibility: Some(Visibility::Public),
        },
    );

    // C imports from B — inherits B's reexport_visibility
    env.add_value_import(
        &mod_c,
        ImportRequest {
            local_name: "helper".to_string(),
            source_module: mod_b.clone(),
            original_name: "helper".to_string(),
            span: dummy_span,
            is_reexport: false,
            reexport_visibility: Some(Visibility::Public),
        },
    );

    // In B: effective = min(Crate, Public) = Crate
    let eff_b = env.effective_value_visibility("helper", Visibility::Crate, &mod_b);
    assert_eq!(eff_b, Visibility::Crate);

    // In C: effective = min(Crate, Public) = Crate (declared vis still Crate)
    let eff_c = env.effective_value_visibility("helper", Visibility::Crate, &mod_c);
    assert_eq!(eff_c, Visibility::Crate);

    // Both within crate: accessible
    assert!(env.is_item_accessible(eff_c, &mod_a, &mod_c, true));
    // Cross-crate: NOT accessible (Crate visibility)
    assert!(!env.is_item_accessible(eff_c, &mod_a, &mod_c, false));
}
