//! Tests for `#[partial]` / `#[decreasing(arg)]` parsing (ADR 29.6.26e).
//!
//! The error paths matter as much as the happy one. An attribute the parser
//! silently swallowed would leave a definition termination-checked while its
//! author believed it had opted out — the one failure mode an escape hatch must
//! not have — so every recovery branch is asserted to *report*, not just to
//! survive.

use super::{parse, parse_ok};
use crate::ast::{Item, TerminationAttrs};

/// The attributes on the first item, which must be a function.
fn attrs_of(source: &str) -> TerminationAttrs {
    let file = parse_ok(source);
    match &file.items[0] {
        Item::Function(func) => func.attrs.clone(),
        other => panic!("expected a function, got {other:?}"),
    }
}

/// The parse errors for `source`.
fn errors_in(source: &str) -> Vec<String> {
    let (_, errors) = parse(source);
    errors.iter().map(ToString::to_string).collect()
}

#[test]
fn a_function_without_attributes_has_none() {
    let attrs = attrs_of("fn f() { 0 }");

    assert!(!attrs.partial);
    assert_eq!(attrs.decreasing, None);
    assert!(attrs.is_empty());
}

#[test]
fn partial_is_recorded() {
    let attrs = attrs_of("#[partial]\nfn f() { 0 }");

    assert!(attrs.partial);
    assert_eq!(attrs.decreasing, None);
    assert!(!attrs.is_empty());
}

#[test]
fn decreasing_records_the_named_parameter() {
    let attrs = attrs_of("#[decreasing(xs)]\nfn f(n: Nat, xs: Nat) { 0 }");

    assert!(!attrs.partial);
    assert!(!attrs.is_empty());
    assert_eq!(
        attrs.decreasing.map(|ident| ident.name),
        Some("xs".to_string())
    );
}

#[test]
fn several_attributes_fold_into_one_record() {
    let attrs = attrs_of("#[partial]\n#[decreasing(xs)]\nfn f(xs: Nat) { 0 }");

    assert!(attrs.partial);
    assert_eq!(
        attrs.decreasing.map(|ident| ident.name),
        Some("xs".to_string())
    );
}

#[test]
fn the_order_the_attributes_are_written_in_does_not_matter() {
    let one_way = attrs_of("#[partial]\n#[decreasing(xs)]\nfn f(xs: Nat) { 0 }");
    let other_way = attrs_of("#[decreasing(xs)]\n#[partial]\nfn f(xs: Nat) { 0 }");

    assert_eq!(one_way.partial, other_way.partial);
    assert_eq!(
        one_way.decreasing.map(|ident| ident.name),
        other_way.decreasing.map(|ident| ident.name)
    );
}

#[test]
fn attributes_come_before_the_visibility_modifier() {
    let attrs = attrs_of("#[partial]\npub fn f() { 0 }");

    assert!(attrs.partial);
    match &parse_ok("#[partial]\npub fn f() { 0 }").items[0] {
        Item::Function(func) => {
            assert_eq!(func.visibility, crate::ast::Visibility::Public);
        }
        other => panic!("expected a function, got {other:?}"),
    }
}

#[test]
fn a_lone_hash_is_not_treated_as_an_attribute() {
    // BOTH `#` and `[` are required to enter the attribute grammar. Without
    // the `[`, the parser must leave the `#` to item dispatch — so the
    // complaint is about the item, and never about an attribute. Asserting only
    // that *some* error appears would not distinguish the two, because the
    // wrong branch errors too, just about the wrong thing.
    let errors = errors_in("# fn f() { 0 }");

    assert!(!errors.is_empty());
    assert!(
        !errors.iter().any(|e| e.contains("known attribute")),
        "a lone `#` must not be parsed as an attribute body: {errors:?}"
    );
    assert!(
        !errors.iter().any(|e| e.contains("after attribute")),
        "a lone `#` must not reach the closing-bracket check: {errors:?}"
    );
}

#[test]
fn an_unknown_attribute_is_reported_rather_than_ignored() {
    let errors = errors_in("#[partal]\nfn f() { 0 }");

    assert!(
        !errors.is_empty(),
        "a misspelled attribute must not be silent"
    );
    assert!(
        errors.iter().any(|e| e.contains("known attribute")),
        "{errors:?}"
    );
}

#[test]
fn an_unknown_attribute_does_not_swallow_the_item_after_it() {
    // Recovery runs to the closing `]`, so the `fn` still parses.
    let (file, _) = parse("#[partal]\nfn f() { 0 }");

    assert_eq!(file.items.len(), 1);
    assert!(matches!(&file.items[0], Item::Function(_)));
}

#[test]
fn decreasing_without_parentheses_is_reported() {
    let errors = errors_in("#[decreasing]\nfn f(xs: Nat) { 0 }");

    assert!(
        errors.iter().any(|e| e.contains("`(` after `decreasing`")),
        "{errors:?}"
    );
}

#[test]
fn decreasing_with_an_unclosed_argument_is_reported() {
    let errors = errors_in("#[decreasing(xs]\nfn f(xs: Nat) { 0 }");

    assert!(
        errors
            .iter()
            .any(|e| e.contains("`)` after the decreasing parameter")),
        "{errors:?}"
    );
}

#[test]
fn an_unclosed_attribute_bracket_is_reported() {
    let errors = errors_in("#[partial\nfn f() { 0 }");

    assert!(
        errors.iter().any(|e| e.contains("`]` after attribute")),
        "{errors:?}"
    );
}

#[test]
fn an_attribute_on_something_other_than_a_function_is_reported() {
    let errors = errors_in("#[partial]\ntype T = A | B");

    assert!(
        errors
            .iter()
            .any(|e| e.contains("`fn` after a termination attribute")),
        "{errors:?}"
    );
}

#[test]
fn a_type_definition_with_no_attributes_is_accepted() {
    // The guard above must fire on the attribute, not on every non-function.
    let file = parse_ok("type T = A | B");

    assert_eq!(file.items.len(), 1);
}

#[test]
fn a_leading_bracket_is_not_an_attribute() {
    // `#` AND `[` are both required. Without the `#` the attribute parser must
    // not consume anything, so the error comes from item dispatch — not from
    // the attribute grammar.
    let errors = errors_in("[foo]\nfn f() { 0 }");

    assert!(!errors.is_empty());
    assert!(
        !errors.iter().any(|e| e.contains("known attribute")),
        "a bare `[` must not be parsed as an attribute body: {errors:?}"
    );
}

#[test]
fn recovery_from_an_unknown_attribute_reports_exactly_one_error() {
    // Recovery stops *at* the `]`, which the caller then eats. Skipping past it
    // — or not skipping at all — adds a spurious second complaint about the
    // bracket, so the count is the assertion that pins the recovery loop.
    let errors = errors_in("#[partal]\nfn f() { 0 }");

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("known attribute"), "{errors:?}");
}

#[test]
fn recovery_from_a_malformed_decreasing_reports_exactly_one_error() {
    let errors = errors_in("#[decreasing(xs]\nfn f(xs: Nat) { 0 }");

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("`)` after the decreasing parameter"),
        "{errors:?}"
    );
}

#[test]
fn recovery_stops_at_the_end_of_input_rather_than_spinning() {
    // No closing `]` anywhere: the loop must terminate on EOF, and the item
    // after it is gone — but the parser returns rather than hanging.
    let errors = errors_in("#[partal");

    assert!(!errors.is_empty(), "{errors:?}");
}

#[test]
fn recovery_skips_trailing_junk_inside_the_brackets() {
    // The recovery loop has to actually move: with junk between the attribute
    // name and the `]`, a parser that did not skip would complain twice — once
    // about the name and once about the bracket it never reached.
    let errors = errors_in("#[partal extra]\nfn f() { 0 }");

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("known attribute"), "{errors:?}");
}
