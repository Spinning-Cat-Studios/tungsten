//! The positivity seam's protocol tests (ADR 18.8.26b AC1).
//!
//! These drive the **externs**, not the registry behind them: the registry's
//! own state machine is exercised through this surface precisely because the
//! marshalling — the sentinel, the C strings, the handle materialisation — is
//! the half a pure-function test cannot reach. A registration path that loses
//! a field produces a definition that passes positivity *because the offending
//! occurrence never arrived*, and no test of the rule can see that.

mod diagnostics;

use std::ffi::{c_char, CStr, CString};

use super::registry::PositivityRegistry;
use super::wire::{def_kind_from_code, DefKind, ProtocolError};
use super::*;
use crate::ffi::types::constructors::{tg_type_arrow, tg_type_nat, tg_type_var};
use crate::ffi::{tg_init, TypeHandle};

/// Kind codes, spelled once so a test reads like the protocol.
const ADT: u64 = 0;
const RECORD: u64 = 1;
const ALIAS: u64 = 2;
const STUB: u64 = 3;

/// A leaked null-terminated C string, as every caller of this seam supplies.
fn cstr(text: &str) -> *const c_char {
    CString::new(text)
        .expect("no interior nul")
        .into_raw()
        .cast_const()
}

/// The positional-field sentinel, as the `.tg` side spells it.
fn positional() -> *const c_char {
    INVALID_HANDLE as *const c_char
}

fn tyvar(name: &str) -> TypeHandle {
    unsafe { tg_type_var(cstr(name)) }
}

// Each test opens by resetting BOTH thread-locals — the arena and the
// registry — inline rather than through a helper. A helper here is unkillable:
// libtest gives each test its own thread, so the resets are redundant under the
// default harness and a `fresh()` whose body vanished would change no verdict.
// Inlining keeps the guard (it is load-bearing the moment a test shares a
// thread) without leaving a mutable site nothing can assert on.

/// The arena's last error message, as the caller reads it.
fn last_error() -> String {
    crate::ffi::ARENA.with(|cell| cell.borrow().last_error.clone())
}

/// The rendered violation, split at the wire format's newline.
fn rendered(index: u64) -> (String, String) {
    let ptr = tg_positivity_violation_render(index);
    assert!(!ptr.is_null(), "violation {index} did not render");
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("rendered violation is UTF-8")
        .to_string();
    let (name, message) = text
        .split_once('\n')
        .expect("wire format is name\\nmessage");
    (name.to_string(), message.to_string())
}

/// `type Bad = Mk(Bad -> Bad)` — the headline rejection, registered through
/// the externs exactly as `elaborate_adt_body` would.
fn register_self_arrow() {
    unsafe {
        assert!(tg_positivity_def_begin(cstr("Bad"), ADT));
        assert!(tg_positivity_ctor_begin(cstr("Mk")));
        let bad = tyvar("Bad");
        let field = tg_type_arrow(bad, bad);
        assert!(tg_positivity_add_field(positional(), field));
    }
}

#[test]
fn the_protocol_rejects_a_self_arrow_registered_through_the_externs() {
    tg_init();
    tg_positivity_reset();
    register_self_arrow();

    assert_eq!(tg_positivity_check(), 1);
    assert_eq!(tg_positivity_violation_count(), 1);
    let (name, message) = rendered(0);
    assert_eq!(name, "Bad");
    assert!(message.contains("is not strictly positive"), "{message}");
    assert!(message.contains("constructor `Mk`"), "{message}");
}

/// D1's implicit-close hazard: there is no `def_end`, so the **last**
/// definition in the stream has no successor to close it. Losing it would
/// leave the corpus one definition short and the analysis silently green.
#[test]
fn the_last_definition_in_the_stream_is_closed_by_check() {
    tg_init();
    tg_positivity_reset();
    unsafe {
        assert!(tg_positivity_def_begin(cstr("Fine"), ADT));
        assert!(tg_positivity_ctor_begin(cstr("MkFine")));
        assert!(tg_positivity_add_field(positional(), tg_type_nat()));
    }
    register_self_arrow();

    assert_eq!(
        tg_positivity_check(),
        1,
        "the definition with no successor `def_begin` was dropped"
    );
    assert_eq!(rendered(0).0, "Bad");
}

/// AC1(a). Handle `0` is a **valid arena index**, so nothing on this seam may
/// read it as absence: a field whose type is handle 0 must contribute its
/// occurrence like any other. `INVALID_HANDLE` is the only sentinel.
#[test]
fn handle_zero_is_a_real_type_not_an_absent_one() {
    tg_init();
    tg_positivity_reset();
    // The first allocation in a fresh arena is handle 0 — assert it, because
    // the whole point is that the sentinel and a live handle can collide.
    let first = tyvar("Bad");
    assert_eq!(first, 0, "handle 0 must be reachable for this test to bite");
    let field = unsafe { tg_type_arrow(first, first) };

    unsafe {
        assert!(tg_positivity_def_begin(cstr("Bad"), ADT));
        assert!(tg_positivity_ctor_begin(cstr("Mk")));
        assert!(tg_positivity_add_field(positional(), field));
    }

    assert_eq!(
        tg_positivity_check(),
        1,
        "a field built from handle 0 was treated as absent"
    );
}

/// The other half of AC1(a): `INVALID_HANDLE` as a *type* is a dangling
/// handle, and registering it fails loudly rather than storing a hole.
#[test]
fn an_invalid_type_handle_fails_rather_than_registering_a_hole() {
    tg_init();
    tg_positivity_reset();
    unsafe {
        assert!(tg_positivity_def_begin(cstr("Bad"), ADT));
        assert!(tg_positivity_ctor_begin(cstr("Mk")));
        assert!(
            !tg_positivity_add_field(positional(), INVALID_HANDLE),
            "a dangling type handle must not register"
        );
    }
    assert_eq!(tg_positivity_check(), 0);
}

/// AC1(b). A positional field is stored **positionally**, not under a name
/// derived from the sentinel — and the index is the field's own position, so a
/// two-field constructor reports `field 1` for its second field.
#[test]
fn the_sentinel_stores_a_field_positionally_at_its_own_index() {
    tg_init();
    tg_positivity_reset();
    unsafe {
        assert!(tg_positivity_def_begin(cstr("Bad"), ADT));
        assert!(tg_positivity_ctor_begin(cstr("Mk")));
        assert!(tg_positivity_add_field(positional(), tg_type_nat()));
        let bad = tyvar("Bad");
        assert!(tg_positivity_add_field(
            positional(),
            tg_type_arrow(bad, bad)
        ));
    }

    assert_eq!(tg_positivity_check(), 1);
    let (_, message) = rendered(0);
    assert!(message.contains("field 1"), "{message}");
    assert!(
        !message.contains("field `"),
        "stored under a name: {message}"
    );
}

/// A record's fields keep their names, which is the only rendering difference
/// records need — and the reason `kind` may not silently default.
#[test]
fn a_record_field_keeps_its_name() {
    tg_init();
    tg_positivity_reset();
    unsafe {
        assert!(tg_positivity_def_begin(cstr("R"), RECORD));
        assert!(tg_positivity_ctor_begin(cstr("R")));
        let r = tyvar("R");
        assert!(tg_positivity_add_field(cstr("loop"), tg_type_arrow(r, r)));
    }

    assert_eq!(tg_positivity_check(), 1);
    let (_, message) = rendered(0);
    assert!(message.contains("record `R`"), "{message}");
    assert!(message.contains("field `loop`"), "{message}");
}

/// An alias is inlined before the walk, so the violation is attributed to the
/// ADT's constructor — an alias has no constructor to point at.
#[test]
fn an_alias_body_is_inlined_into_the_definition_that_uses_it() {
    tg_init();
    tg_positivity_reset();
    unsafe {
        assert!(tg_positivity_def_begin(cstr("F"), ALIAS));
        assert!(tg_positivity_add_param(cstr("T")));
        let arrow = tg_type_arrow(tyvar("T"), tg_type_nat());
        assert!(tg_positivity_set_alias_body(arrow));

        assert!(tg_positivity_def_begin(cstr("Bad4"), ADT));
        assert!(tg_positivity_ctor_begin(cstr("B")));
        // `F<Bad4>` reaches the seam as the already-substituted body, which is
        // what the self-hosted elaborator produces: it instantiates generics
        // structurally rather than leaving an `App` head.
        let instantiated = tg_type_arrow(tyvar("Bad4"), tg_type_nat());
        assert!(tg_positivity_add_field(positional(), instantiated));
    }

    assert_eq!(tg_positivity_check(), 1);
    assert_eq!(rendered(0).0, "Bad4");
}

/// A stub's field types are lossy, so it is skipped rather than doubted — and
/// it must not become a checkable definition with no constructors.
#[test]
fn a_stub_registers_as_a_stub_and_is_not_checked() {
    tg_init();
    tg_positivity_reset();
    unsafe {
        assert!(tg_positivity_def_begin(cstr("Imported"), STUB));
    }
    assert_eq!(tg_positivity_check(), 0);
}

/// Out-of-order calls are errors, never silent skips. Each of these would
/// otherwise drop data on the floor and produce a *green* analysis.
#[test]
fn out_of_order_calls_fail_rather_than_being_ignored() {
    tg_init();
    tg_positivity_reset();
    unsafe {
        assert!(!tg_positivity_add_param(cstr("T")), "param with no def");
        assert!(!tg_positivity_ctor_begin(cstr("Mk")), "ctor with no def");
        assert!(
            !tg_positivity_add_field(positional(), tg_type_nat()),
            "field with no ctor"
        );
        assert!(
            !tg_positivity_set_alias_body(tg_type_nat()),
            "alias body with no def"
        );

        assert!(tg_positivity_def_begin(cstr("T"), ADT));
        assert!(tg_positivity_ctor_begin(cstr("MkT")));
        assert!(
            !tg_positivity_add_param(cstr("A")),
            "a parameter arriving after the first constructor is out of order"
        );
        assert!(
            !tg_positivity_set_alias_body(tg_type_nat()),
            "an ADT has no alias body"
        );

        assert!(tg_positivity_def_begin(cstr("Al"), ALIAS));
        assert!(
            !tg_positivity_ctor_begin(cstr("Nope")),
            "an alias has no constructors"
        );
    }
}

/// An unrecognised `kind` is a hard error. Defaulting it would turn a record
/// into an ADT and change the answer.
#[test]
fn an_unknown_kind_code_is_refused() {
    tg_init();
    tg_positivity_reset();
    assert!(!unsafe { tg_positivity_def_begin(cstr("X"), 4) });
    assert_eq!(def_kind_from_code(4), None);
    assert_eq!(def_kind_from_code(0), Some(DefKind::Adt));
    assert_eq!(def_kind_from_code(1), Some(DefKind::Record));
    assert_eq!(def_kind_from_code(2), Some(DefKind::Alias));
    assert_eq!(def_kind_from_code(3), Some(DefKind::Stub));
}

/// Checking twice in one pass must give the same answer. `check` consumes the
/// definition map to build [`PositivityDefs`], so a driver that forgot to put
/// it back would report a corpus of zero on the second call.
#[test]
fn checking_twice_does_not_empty_the_environment() {
    tg_init();
    tg_positivity_reset();
    register_self_arrow();

    assert_eq!(tg_positivity_check(), 1);
    assert_eq!(tg_positivity_check(), 1, "the environment was consumed");
}

/// The registry starts empty, so "no violations" and "nothing registered" are
/// both zero — which is why the `.tg` caller reports on the *count*, and the
/// gate's own vacuity is the conformance harness's job rather than this
/// seam's.
#[test]
fn a_fresh_registry_has_nothing_to_report() {
    let mut registry = PositivityRegistry::default();
    assert_eq!(registry.check(), 0);
    assert_eq!(registry.violation_count(), 0);
    assert!(registry.violation_at(0).is_none());
}

/// AC1(a), the null half. `INVALID_HANDLE` is the ONLY positional spelling: a
/// null field name fails rather than quietly becoming an unnamed field, because
/// `0` is a valid arena index and a seam that accepted it as "absent" would
/// turn a lost name into a positional field instead of an error.
#[test]
fn a_null_field_name_is_refused_rather_than_read_as_positional() {
    tg_init();
    tg_positivity_reset();
    unsafe {
        assert!(tg_positivity_def_begin(cstr("R"), RECORD));
        assert!(tg_positivity_ctor_begin(cstr("R")));
        assert!(
            !tg_positivity_add_field(std::ptr::null(), tg_type_nat()),
            "null is not a second spelling of the positional sentinel"
        );
    }
    assert!(last_error().contains("field name"), "{}", last_error());
    assert_eq!(tg_positivity_check(), 0, "the field was not registered");
}
