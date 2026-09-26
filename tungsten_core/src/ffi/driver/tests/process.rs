//! Process FFI: subprocess execution, environment, tty detection.

use std::ffi::CString;

use crate::ffi::driver::console::{tg_exec_process, tg_getenv, tg_stderr_is_tty, tg_stdout_is_tty};
use crate::ffi::driver::io::tg_cstring_to_string;
use crate::ffi::driver::tg_free_string;

/// 14.9.26c AC 3: the line `tg_int_trap` prints is the evaluator's line for
/// the same stop, code for code — the parity the `diff exec` fixture relies on.
#[test]
fn int_trap_line_matches_the_evaluator_message_for_every_code() {
    use crate::eval::{EvalStopped, IntTrapKind};
    use crate::ffi::driver::console::int_trap_line;
    for code in 0..8 {
        let kind = IntTrapKind::from_code(code).expect("dense table");
        // `tungsten run` prints `error: {stopped}` — the prefix is part of the line.
        let rendered = format!("error: {}", EvalStopped::IntTrap { kind });
        assert_eq!(int_trap_line(code), rendered, "code {code}");
    }
    assert_eq!(int_trap_line(0), "error: integer overflow in +");
    assert_eq!(int_trap_line(42), "error: integer trap (unknown kind 42)");
}

#[test]
fn test_tty_detection() {
    // These should return 0 or 1 without crashing
    let stdout_tty = tg_stdout_is_tty();
    let stderr_tty = tg_stderr_is_tty();
    assert!(stdout_tty == 0 || stdout_tty == 1);
    assert!(stderr_tty == 0 || stderr_tty == 1);
}

#[test]
fn test_exec_process_success() {
    let program = CString::new("echo").unwrap();
    let args = CString::new("hello").unwrap();
    let code = tg_exec_process(program.as_ptr(), args.as_ptr());
    assert_eq!(code, 0);
}

#[test]
fn test_exec_process_failure() {
    let program = CString::new("false").unwrap();
    let args = CString::new("").unwrap();
    let code = tg_exec_process(program.as_ptr(), args.as_ptr());
    assert_ne!(code, 0);
}

#[test]
fn test_exec_process_nonexistent() {
    let program = CString::new("/nonexistent/binary/xxxxx").unwrap();
    let args = CString::new("").unwrap();
    let code = tg_exec_process(program.as_ptr(), args.as_ptr());
    assert_eq!(code, -1);
}

#[test]
fn test_exec_process_multi_args() {
    // echo with multiple args separated by newlines
    let program = CString::new("echo").unwrap();
    let args = CString::new("arg1\narg2\narg3").unwrap();
    let code = tg_exec_process(program.as_ptr(), args.as_ptr());
    assert_eq!(code, 0);
}

#[test]
fn test_exec_process_null_args() {
    let program = CString::new("true").unwrap();
    let code = tg_exec_process(program.as_ptr(), std::ptr::null());
    assert_eq!(code, 0);
}

#[test]
fn test_getenv_exists() {
    // PATH should always be set
    let name = CString::new("PATH").unwrap();
    let result = tg_getenv(name.as_ptr());
    assert!(!result.is_null());
    // Clean up
    tg_free_string(result);
}

#[test]
fn test_getenv_not_set() {
    let name = CString::new("TUNGSTEN_TEST_NONEXISTENT_VAR_12345").unwrap();
    let result = tg_getenv(name.as_ptr());
    assert!(result.is_null());
}
