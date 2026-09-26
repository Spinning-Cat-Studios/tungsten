//! What the caller reads back from the seam (ADR 18.8.26b D1/D6).
//!
//! Split from the protocol tests next door along the seam the module already
//! has: those pin which calls the grammar allows, these pin what a refused call
//! and a found violation LEAVE BEHIND. The distinction is not cosmetic — a
//! `fail` that returns `false` without recording a reason passes every protocol
//! test and tells the caller nothing about which rule it broke.

use super::*;

/// Every protocol error carries a distinct message, so `tg_get_last_error`
/// names which rule was broken rather than only that one was.
#[test]
fn protocol_error_messages_are_distinct() {
    let messages = [
        ProtocolError::UnknownKind(9).message(),
        ProtocolError::NoCurrentDefinition("add_param").message(),
        ProtocolError::NoCurrentConstructor.message(),
        ProtocolError::ParamAfterConstructor.message(),
        ProtocolError::ConstructorOnNonAdt.message(),
        ProtocolError::AliasBodyOnNonAlias.message(),
    ];
    let distinct: std::collections::BTreeSet<&String> = messages.iter().collect();
    assert_eq!(distinct.len(), messages.len(), "{messages:?}");
    assert!(messages[0].contains('9'), "the offending code is named");
}

/// `reset` clears the violations too, not only the definitions: a reader
/// between two passes must not serve the previous pass's answer.
#[test]
fn reset_clears_the_previous_passs_violations() {
    tg_init();
    tg_positivity_reset();
    register_self_arrow();
    assert_eq!(tg_positivity_check(), 1);

    tg_positivity_reset();

    assert_eq!(tg_positivity_violation_count(), 0);
    assert!(tg_positivity_violation_render(0).is_null());
}

/// An out-of-range index renders null rather than panicking or wrapping: the
/// caller bounds its loop with the count, and a null is how it learns it did
/// not.
#[test]
fn an_out_of_range_violation_index_renders_null() {
    tg_init();
    tg_positivity_reset();
    register_self_arrow();
    assert_eq!(tg_positivity_check(), 1);

    assert!(tg_positivity_violation_render(1).is_null());
    assert!(tg_positivity_violation_render(u64::MAX).is_null());
}

/// Survivor-killer for `fail`: a protocol failure must leave its reason where
/// `tg_get_last_error` finds it. Returning `false` and recording nothing is a
/// gate that refuses a call without saying which rule it broke — and no other
/// test here reads the message, so a `fail` that stopped writing one would look
/// identical to one that did.
#[test]
fn a_refused_call_records_its_reason_for_the_caller_to_read() {
    tg_init();
    tg_positivity_reset();
    assert_eq!(last_error(), "", "a fresh arena carries no error");

    assert!(!unsafe { tg_positivity_def_begin(cstr("X"), 4) });

    let message = last_error();
    assert!(message.contains("unknown definition kind"), "{message}");
    assert!(
        message.contains('4'),
        "the offending code is named: {message}"
    );
}

/// The distinct-message property, through the seam rather than over the enum:
/// two different protocol breaches must not read alike to the caller.
#[test]
fn two_different_breaches_leave_two_different_reasons() {
    tg_init();
    tg_positivity_reset();
    unsafe {
        assert!(!tg_positivity_add_param(cstr("T")));
        let no_definition = last_error();
        assert!(!tg_positivity_add_field(positional(), tg_type_nat()));
        let no_constructor = last_error();
        assert_ne!(no_definition, no_constructor);
        assert!(!no_definition.is_empty());
    }
}
