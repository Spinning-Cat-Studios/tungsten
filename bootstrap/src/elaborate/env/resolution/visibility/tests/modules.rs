use super::*;

#[test]
fn test_is_module_accessible_root_always_accessible() {
    let env = Env::new();
    let root = ModulePath::root();
    let foo = ModulePath::from_name("foo");

    // Root module is always accessible
    assert!(env.is_module_accessible(&root, &foo, true));
    assert!(env.is_module_accessible(&root, &root, true));
}

#[test]
fn test_is_module_accessible_public_module() {
    let mut env = Env::new();
    let root = ModulePath::root();
    let foo = ModulePath::from_name("foo");
    let bar = ModulePath::from_name("bar");

    // Register foo as a public module under root
    env.register_module_with_visibility(foo.clone(), Visibility::Public, Some(root.clone()));

    // Public module is accessible from anywhere in the same crate
    assert!(env.is_module_accessible(&foo, &root, true));
    assert!(env.is_module_accessible(&foo, &bar, true));

    // Not accessible from different crate
    assert!(!env.is_module_accessible(&foo, &bar, false));
}

#[test]
fn test_is_module_accessible_private_module_from_parent() {
    let mut env = Env::new();
    let root = ModulePath::root();
    let foo = ModulePath::from_name("foo");

    // Register foo as a private module under root
    env.register_module_with_visibility(foo.clone(), Visibility::Private, Some(root.clone()));

    // Private module accessible from parent (root)
    assert!(env.is_module_accessible(&foo, &root, true));
}

#[test]
fn test_is_module_accessible_private_module_from_sibling_parent() {
    let mut env = Env::new();
    let root = ModulePath::root();
    let foo = ModulePath::from_name("foo");
    let bar = ModulePath::from_name("bar");

    // Register foo as private under root
    env.register_module_with_visibility(foo.clone(), Visibility::Private, Some(root.clone()));
    // Register bar as public under root
    env.register_module_with_visibility(bar.clone(), Visibility::Public, Some(root.clone()));

    // foo is accessible from bar (because bar is also under root, which is foo's parent)
    // In Rust: siblings can access private siblings through the parent
    assert!(env.is_module_accessible(&foo, &bar, true));
}

#[test]
fn test_is_module_accessible_private_nested_module() {
    let mut env = Env::new();
    let root = ModulePath::root();
    let foo = ModulePath::from_name("foo");
    let foo_bar = foo.child("bar");
    let other = ModulePath::from_name("other");

    // Register foo as public under root
    env.register_module_with_visibility(foo.clone(), Visibility::Public, Some(root.clone()));
    // Register foo::bar as private under foo
    env.register_module_with_visibility(foo_bar.clone(), Visibility::Private, Some(foo.clone()));

    // foo::bar is accessible from foo (the parent)
    assert!(env.is_module_accessible(&foo_bar, &foo, true));

    // foo::bar is NOT accessible from other (not a descendant of foo)
    assert!(!env.is_module_accessible(&foo_bar, &other, true));

    // foo::bar is NOT accessible from root (not a descendant of foo, only of foo::bar)
    // Actually, root IS NOT a descendant of foo, so it should be false
    assert!(!env.is_module_accessible(&foo_bar, &root, true));
}

#[test]
fn test_is_module_accessible_unknown_module() {
    let env = Env::new();
    let unknown = ModulePath::from_name("unknown");
    let foo = ModulePath::from_name("foo");

    // Unknown modules are accessible (for bootstrapping)
    assert!(env.is_module_accessible(&unknown, &foo, true));
}
