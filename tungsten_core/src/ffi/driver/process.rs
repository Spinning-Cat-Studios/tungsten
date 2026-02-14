//! Process control, TTY detection, subprocess execution, and environment FFI functions

use std::ffi::{c_char, CStr, CString};
use std::process::Command;
use std::ptr;

use super::{clear_driver_error, set_driver_error};

extern "C" {
    /// C library's _Exit function - immediate termination without cleanup
    fn _Exit(status: i32) -> !;
}

/// Exit the process with given code.
///
/// This function does not return.
/// Uses C's _`Exit()` for immediate termination without running cleanup handlers,
/// avoiding issues with Rust's atexit handlers or other cleanup code.
#[no_mangle]
pub extern "C" fn tg_exit(code: i32) -> ! {
    unsafe { _Exit(code) }
}

// ============================================================================
// TTY Detection (for colored output)
// ============================================================================

/// Check if stdout supports color output (is a TTY).
///
/// Returns 1 if TTY, 0 if not.
#[no_mangle]
pub extern "C" fn tg_stdout_is_tty() -> i32 {
    i32::from(atty::is(atty::Stream::Stdout))
}

/// Check if stderr supports color output (is a TTY).
///
/// Returns 1 if TTY, 0 if not.
#[no_mangle]
pub extern "C" fn tg_stderr_is_tty() -> i32 {
    i32::from(atty::is(atty::Stream::Stderr))
}

// ============================================================================
// Subprocess Execution
// ============================================================================

/// Execute a program with arguments.
///
/// `program` is the executable path (null-terminated C string).
/// `args` is a newline-delimited C string of arguments (each line is one arg).
///   An empty string means no arguments.
///
/// The subprocess inherits stdout/stderr, so its output flows through naturally.
///
/// Returns the process exit code (0 = success), or -1 if the process fails to
/// spawn. On spawn failure, call `tg_driver_get_last_error` for details.
///
/// All arguments are constructed from trusted paths; no user input is interpolated.
#[no_mangle]
pub extern "C" fn tg_exec_process(program: *const c_char, args: *const c_char) -> i64 {
    clear_driver_error();

    if program.is_null() {
        set_driver_error("program path is null");
        return -1;
    }

    let program_str = match unsafe { CStr::from_ptr(program) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_driver_error(format!("invalid program path: {e}"));
            return -1;
        }
    };

    let mut cmd = Command::new(program_str);

    // Parse newline-delimited arguments
    if !args.is_null() {
        if let Ok(args_str) = unsafe { CStr::from_ptr(args) }.to_str() {
            for arg in args_str.split('\n') {
                if !arg.is_empty() {
                    cmd.arg(arg);
                }
            }
        }
    }

    match cmd.status() {
        Ok(status) => i64::from(status.code().unwrap_or(-1)),
        Err(e) => {
            set_driver_error(format!("failed to execute '{program_str}': {e}"));
            -1
        }
    }
}

// ============================================================================
// Current Executable Directory
// ============================================================================

/// Get the directory containing the current executable.
///
/// Returns the directory path as a null-terminated C string (caller must free
/// with `tg_free_string`), or null if the path cannot be determined.
#[no_mangle]
pub extern "C" fn tg_current_exe_dir() -> *mut c_char {
    clear_driver_error();

    match std::env::current_exe() {
        Ok(exe_path) => {
            if let Some(dir) = exe_path.parent() {
                let dir_str = dir.to_string_lossy();
                CString::new(dir_str.as_ref())
                    .map(std::ffi::CString::into_raw)
                    .unwrap_or(ptr::null_mut())
            } else {
                set_driver_error("current executable has no parent directory");
                ptr::null_mut()
            }
        }
        Err(e) => {
            set_driver_error(format!("failed to get current executable path: {e}"));
            ptr::null_mut()
        }
    }
}

// ============================================================================
// Environment Variables
// ============================================================================

/// Get the value of an environment variable.
///
/// Returns the value as a null-terminated C string (caller must free with
/// `tg_free_string`), or null if the variable is not set.
#[no_mangle]
pub extern "C" fn tg_getenv(name: *const c_char) -> *mut c_char {
    clear_driver_error();

    if name.is_null() {
        return ptr::null_mut();
    }

    let name_str = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    match std::env::var(name_str) {
        Ok(val) => CString::new(val)
            .map(std::ffi::CString::into_raw)
            .unwrap_or(ptr::null_mut()),
        Err(_) => ptr::null_mut(),
    }
}
