use super::*;

#[test]
fn test_min_visibility_same() {
    assert_eq!(
        Env::min_visibility(Visibility::Public, Visibility::Public),
        Visibility::Public
    );
    assert_eq!(
        Env::min_visibility(Visibility::Crate, Visibility::Crate),
        Visibility::Crate
    );
    assert_eq!(
        Env::min_visibility(Visibility::Private, Visibility::Private),
        Visibility::Private
    );
}

#[test]
fn test_min_visibility_picks_more_restrictive() {
    assert_eq!(
        Env::min_visibility(Visibility::Public, Visibility::Crate),
        Visibility::Crate
    );
    assert_eq!(
        Env::min_visibility(Visibility::Crate, Visibility::Public),
        Visibility::Crate
    );
    assert_eq!(
        Env::min_visibility(Visibility::Public, Visibility::Private),
        Visibility::Private
    );
    assert_eq!(
        Env::min_visibility(Visibility::Private, Visibility::Public),
        Visibility::Private
    );
    assert_eq!(
        Env::min_visibility(Visibility::Crate, Visibility::Private),
        Visibility::Private
    );
    assert_eq!(
        Env::min_visibility(Visibility::Private, Visibility::Crate),
        Visibility::Private
    );
}
