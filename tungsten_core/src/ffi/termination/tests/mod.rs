//! The termination seam's protocol tests (ADR 19.8.26d).
//!
//! These drive the **externs**, not the registry behind them: the marshalling —
//! the C strings, the handle materialisation, the phase the call lands in — is
//! the half a pure-function test cannot reach, and a registration path that
//! loses a definition produces a program that passes the termination gate
//! *because the offending recursion never arrived*.
//!
//! The suite is split three ways by the question each part asks. **Here**: the
//! protocol — whether a call is allowed where it landed, and whether a refusal
//! says why. [`verdicts`]: what the analysis concludes and how it renders.
//! [`agreement`]: the claim this ADR's design rests on, that reducing most
//! definitions and retaining only the recursive ones reaches the same verdict
//! as handing the engine everything.

mod agreement;
mod verdicts;

use std::ffi::{c_char, CStr, CString};

use super::registry::Phase;
use super::*;
use crate::ffi::terms::core::{tg_term_app, tg_term_global};
use crate::ffi::terms::core_data::tg_term_zero;
use crate::ffi::types::constructors::{tg_type_arrow, tg_type_nat};
use crate::ffi::{tg_init, TermHandle, TypeHandle};

/// A leaked null-terminated C string, as every caller of this seam supplies.
fn cstr(text: &str) -> *const c_char {
    CString::new(text)
        .expect("no interior nul")
        .into_raw()
        .cast_const()
}

// Each test opens by resetting BOTH thread-locals — the arena and the registry
// — inline rather than through a helper, for the reason ADR 18.8.26b's sweep
// established: libtest gives each test its own thread, so a `fresh()` helper is
// an extraction no assertion can distinguish a mutation of.

/// The arena's last error message, as the caller reads it.
fn last_error() -> String {
    crate::ffi::ARENA.with(|cell| cell.borrow().last_error.clone())
}

/// `fn <name>(_) = <name>(0)` — a self-call that reconstructs rather than
/// descends, which is the shape the gate refuses.
fn self_call(name: &str) -> TermHandle {
    unsafe { tg_term_app(tg_term_global(cstr(name)), tg_term_zero()) }
}

/// `Nat -> Nat`, a type carrying no embedded terms.
fn nat_to_nat() -> TypeHandle {
    tg_type_arrow(tg_type_nat(), tg_type_nat())
}

/// The rendered rejection, split into its three wire fields.
fn rendered(index: u64) -> (String, String, String) {
    let ptr = tg_termination_failure_render(index);
    assert!(!ptr.is_null(), "failure {index} did not render");
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("utf-8")
        .to_string();
    let mut parts = text.splitn(3, '\n');
    (
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
    )
}

#[test]
fn an_undeclared_definition_is_refused_and_says_why() {
    tg_init();
    tg_termination_reset();
    let accepted =
        unsafe { tg_termination_add_def(cstr("stranger"), nat_to_nat(), tg_term_zero()) };
    assert!(!accepted, "an undeclared name is not a new node");
    assert!(
        last_error().contains("never declared"),
        "the message names the rule: {}",
        last_error()
    );
}
#[test]
fn declaring_after_the_plan_is_refused() {
    tg_init();
    tg_termination_reset();
    tg_termination_plan(true);
    let accepted = unsafe { tg_termination_declare(cstr("late")) };
    assert!(!accepted);
    assert!(
        last_error().contains("retain"),
        "the message names the phase it was in: {}",
        last_error()
    );
}
#[test]
fn checking_before_planning_is_refused_rather_than_answered() {
    tg_init();
    tg_termination_reset();
    unsafe {
        tg_termination_declare(cstr("loop"));
        tg_termination_add_def(cstr("loop"), nat_to_nat(), self_call("loop"));
    }
    assert_eq!(
        tg_termination_check(),
        0,
        "a check that never planned reports no rejections"
    );
    assert!(
        last_error().contains("`check` is not allowed"),
        "and says so rather than looking clean: {}",
        last_error()
    );
}
#[test]
fn an_unreadable_name_is_refused_on_every_registration_extern() {
    tg_init();
    tg_termination_reset();
    let null = std::ptr::null();
    unsafe {
        assert!(!tg_termination_declare(null));
        assert!(last_error().contains("declare"));
        assert!(!tg_termination_note_item(null, false, null, false));
        assert!(last_error().contains("note_item"));
        assert!(!tg_termination_add_def(null, nat_to_nat(), tg_term_zero()));
        assert!(last_error().contains("add_def"));
    }
}
#[test]
fn a_dangling_handle_is_refused_rather_than_dropped() {
    tg_init();
    tg_termination_reset();
    unsafe {
        tg_termination_declare(cstr("d"));
        assert!(!tg_termination_add_def(
            cstr("d"),
            crate::ffi::INVALID_HANDLE,
            tg_term_zero()
        ));
    }
    assert!(
        last_error().contains("names nothing in the arena"),
        "{}",
        last_error()
    );
}
#[test]
fn reset_returns_the_registry_to_the_reduce_phase() {
    tg_init();
    tg_termination_reset();
    tg_termination_plan(true);
    tg_termination_reset();
    assert!(unsafe { tg_termination_declare(cstr("again")) });
    assert_eq!(
        with_registry(|registry| registry.phase()),
        Phase::Reduce,
        "a fresh pass starts where the protocol starts"
    );
}
#[test]
fn an_out_of_range_failure_index_renders_null() {
    tg_init();
    tg_termination_reset();
    assert!(tg_termination_failure_render(0).is_null());
}
#[test]
fn a_definition_offered_after_the_check_is_refused_and_names_the_phase() {
    // The one phase transition nothing else reaches. It matters because the
    // analysis has already run: a definition accepted here would sit in the
    // registry unexamined, and the NEXT check — over a registry the caller
    // believes it reset — would silently include it.
    tg_init();
    tg_termination_reset();
    unsafe {
        tg_termination_declare(cstr("d"));
        tg_termination_add_def(cstr("d"), nat_to_nat(), tg_term_zero());
    }
    tg_termination_plan(true);
    tg_termination_check();
    let accepted = unsafe { tg_termination_add_def(cstr("d"), nat_to_nat(), tg_term_zero()) };
    assert!(!accepted);
    assert!(
        last_error().contains("after the analysis already ran"),
        "{}",
        last_error()
    );
    assert!(
        !unsafe { tg_termination_declare(cstr("late")) },
        "and the checked phase refuses a declaration too"
    );
    assert!(last_error().contains("checked"), "{}", last_error());
}
