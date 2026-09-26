//! Tests for the console capture sink (ADR 28.7.26a AC 5).
//!
//! The sink is process-global mutable state, so these tests must not run
//! concurrently with each other — nor with the console-extern tests in
//! `eval/env/handlers/extern_console_tests.rs`, which drive the same sink from
//! the same test binary. Both serialize on the ONE lock declared beside the
//! resource, [`super::test_exclusive::exclusive_sink`]; see its docs for the
//! race that a per-file lock caused.

use std::ffi::c_char;

use super::super::console::reported_tty;
use super::test_exclusive::exclusive_sink;
use super::{install, is_active, take, CapturedOutput, InstallError};
use crate::ffi::{tg_eprintln, tg_print, tg_println, tg_stderr_is_tty, tg_stdout_is_tty};

/// Print `text` through the real `tg_println` FFI entry point, the way
/// evaluated Tungsten code reaches the sink.
fn println_via_ffi(text: &str) {
    tg_println(text.as_ptr().cast::<c_char>(), text.len() as u64);
}

/// Print `text` through the real `tg_eprintln` FFI entry point.
fn eprintln_via_ffi(text: &str) {
    tg_eprintln(text.as_ptr().cast::<c_char>(), text.len() as u64);
}

/// Print `text` through the real `tg_print` FFI entry point (no newline).
fn print_via_ffi(text: &str) {
    tg_print(text.as_ptr().cast::<c_char>(), text.len() as u64);
}

/// `tg_print` captures without appending a newline — the distinction from
/// `tg_println`, and the reason both route through the sink separately.
#[test]
fn print_captures_without_a_newline() {
    let _guard = exclusive_sink();

    install().expect("no sink should be installed");
    print_via_ffi("bare");
    let captured = take().expect("a sink was installed");

    assert_eq!(captured.stdout, b"bare");
}

/// `tg_print` on a null pointer writes nothing at all — not even a newline,
/// unlike `tg_println`. Pins the asymmetry so capture cannot "helpfully" add
/// one.
#[test]
fn print_of_null_captures_nothing() {
    let _guard = exclusive_sink();

    install().expect("no sink should be installed");
    tg_print(std::ptr::null(), 0);
    let captured = take().expect("a sink was installed");

    assert!(
        captured.stdout.is_empty(),
        "tg_print(null) should write nothing, got {:?}",
        captured.stdout
    );
}

/// AC 5(a): with a sink installed, printed bytes land in the stdout buffer —
/// and the stderr buffer stays empty, since the two must not be merged at the
/// source.
#[test]
fn println_lands_in_the_stdout_buffer_only() {
    let _guard = exclusive_sink();

    install().expect("no sink should be installed");
    println_via_ffi("hello");
    let captured = take().expect("a sink was installed");

    assert_eq!(captured.stdout, b"hello\n");
    assert!(
        captured.stderr.is_empty(),
        "stderr buffer should be untouched, got {:?}",
        captured.stderr
    );
}

/// AC 5(b): `tg_eprintln` lands in the stderr buffer, not stdout.
#[test]
fn eprintln_lands_in_the_stderr_buffer_only() {
    let _guard = exclusive_sink();

    install().expect("no sink should be installed");
    eprintln_via_ffi("boom");
    let captured = take().expect("a sink was installed");

    assert_eq!(captured.stderr, b"boom\n");
    assert!(
        captured.stdout.is_empty(),
        "stdout buffer should be untouched, got {:?}",
        captured.stdout
    );
}

/// AC 5(c): with no sink installed, output still reaches the process stream.
///
/// The stream write itself cannot be observed from in-process, so this asserts
/// the falsifiable half: nothing is captured, and the call still returns. A
/// regression that routed uninstalled writes into a buffer would fail here.
#[test]
fn uninstalled_writes_are_not_captured() {
    let _guard = exclusive_sink();

    assert!(!is_active(), "sink should start uninstalled");
    println_via_ffi("to the real stdout");
    eprintln_via_ffi("to the real stderr");

    assert!(take().is_none(), "nothing should have been captured");
}

/// Multiple writes accumulate in order within a single capture.
#[test]
fn writes_accumulate_in_order() {
    let _guard = exclusive_sink();

    install().expect("no sink should be installed");
    println_via_ffi("first");
    eprintln_via_ffi("warned");
    println_via_ffi("second");
    let captured = take().expect("a sink was installed");

    assert_eq!(captured.stdout, b"first\nsecond\n");
    assert_eq!(captured.stderr, b"warned\n");
}

/// A null pointer prints a bare newline — the pre-existing `tg_println`
/// contract, which capture must reproduce rather than drop.
#[test]
fn null_pointer_captures_a_bare_newline() {
    let _guard = exclusive_sink();

    install().expect("no sink should be installed");
    tg_println(std::ptr::null(), 0);
    tg_eprintln(std::ptr::null(), 0);
    let captured = take().expect("a sink was installed");

    assert_eq!(captured.stdout, b"\n");
    assert_eq!(captured.stderr, b"\n");
}

/// `install` reports a double-install rather than silently interleaving two
/// runs' output into one buffer.
#[test]
fn double_install_is_rejected() {
    let _guard = exclusive_sink();

    install().expect("no sink should be installed");
    assert_eq!(install(), Err(InstallError::AlreadyInstalled));

    let _ = take();
}

/// `take` with nothing installed yields `None` and leaves the sink inactive.
#[test]
fn take_without_install_yields_none() {
    let _guard = exclusive_sink();

    assert!(take().is_none());
    assert!(!is_active());
}

/// `is_active` tracks install/take, since it is the flag every console write
/// branches on.
#[test]
fn is_active_tracks_install_and_take() {
    let _guard = exclusive_sink();

    assert!(!is_active());
    install().expect("no sink should be installed");
    assert!(is_active());
    let _ = take();
    assert!(!is_active());
}

/// A second capture starts empty — `install` must not resurrect the previous
/// run's bytes.
#[test]
fn a_fresh_install_starts_empty() {
    let _guard = exclusive_sink();

    install().expect("no sink should be installed");
    println_via_ffi("run one");
    let _ = take();

    install().expect("the sink was released");
    let captured = take().expect("a sink was installed");
    assert_eq!(captured, CapturedOutput::default());
}

/// Under capture, both TTY probes report "not a terminal", so nothing emits
/// colour escapes into a buffer that would render them literally.
#[test]
fn tty_probes_report_false_under_capture() {
    let _guard = exclusive_sink();

    install().expect("no sink should be installed");
    assert_eq!(tg_stdout_is_tty(), 0);
    assert_eq!(tg_stderr_is_tty(), 0);
    let _ = take();
}

/// Capture suppresses the TTY report even for a stream that genuinely IS a
/// terminal — the case the FFI entry points cannot exercise, since a test
/// harness's stdout is never one.
#[test]
fn capture_suppresses_a_real_terminal() {
    let _guard = exclusive_sink();

    install().expect("no sink should be installed");
    assert_eq!(reported_tty(true), 0, "capture must win over a real tty");
    assert_eq!(reported_tty(false), 0);
    let _ = take();
}

/// With no sink installed, the real stream state is reported through
/// unchanged — capture must not suppress colour for ordinary native runs.
#[test]
fn without_capture_the_real_stream_state_is_reported() {
    let _guard = exclusive_sink();

    assert!(!is_active());
    assert_eq!(reported_tty(true), 1, "a real tty must still report as one");
    assert_eq!(reported_tty(false), 0);
}

/// The `InstallError` display string names the condition, since it surfaces to
/// a facade caller that cannot see this module.
#[test]
fn install_error_displays_the_condition() {
    assert_eq!(
        InstallError::AlreadyInstalled.to_string(),
        "a console capture sink is already installed"
    );
}
