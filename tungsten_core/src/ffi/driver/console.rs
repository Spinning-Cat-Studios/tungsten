//! Console, CLI, and process control FFI functions.
//!
//! Terminal I/O: print, println, eprint, debug_tag
//! CLI arguments: init_args, argc, argv
//! Process control: exit, TTY detection, subprocess execution, environment

use std::ffi::{c_char, CStr, CString};
use std::io::{self, Write};
use std::process::Command;
use std::ptr;
use std::sync::OnceLock;

use super::{clear_driver_error, set_driver_error};

// ============================================================================
// Console Output
// ============================================================================

/// Print to stdout (no newline).
///
/// # Safety
/// - `s` must be a valid pointer to `len` bytes
#[no_mangle]
pub extern "C" fn tg_print(s: *const c_char, len: u64) {
    if s.is_null() {
        return;
    }

    let slice = unsafe { std::slice::from_raw_parts(s.cast::<u8>(), len as usize) };
    let _ = io::stdout().write_all(slice);
    let _ = io::stdout().flush();
}

/// Print to stdout with newline.
///
/// # Safety
/// - `s` must be a valid pointer to `len` bytes
#[no_mangle]
pub extern "C" fn tg_println(s: *const c_char, len: u64) {
    if s.is_null() {
        let _ = io::stdout().write_all(b"\n");
        let _ = io::stdout().flush();
        return;
    }

    let slice = unsafe { std::slice::from_raw_parts(s.cast::<u8>(), len as usize) };
    let _ = io::stdout().write_all(slice);
    let _ = io::stdout().write_all(b"\n");
    let _ = io::stdout().flush();
}

/// Print to stderr with newline.
///
/// # Safety
/// - `s` must be a valid pointer to `len` bytes
#[no_mangle]
pub extern "C" fn tg_eprintln(s: *const c_char, len: u64) {
    if s.is_null() {
        let _ = io::stderr().write_all(b"\n");
        let _ = io::stderr().flush();
        return;
    }

    let slice = unsafe { std::slice::from_raw_parts(s.cast::<u8>(), len as usize) };
    let _ = io::stderr().write_all(slice);
    let _ = io::stderr().write_all(b"\n");
    let _ = io::stderr().flush();
}

/// Debug print a u8 value to stderr.
/// Used for tracing tag values in sum types.
#[no_mangle]
pub extern "C" fn tg_debug_tag(label: *const c_char, label_len: u64, tag: u8) {
    let label_slice = if label.is_null() {
        b"tag" as &[u8]
    } else {
        unsafe { std::slice::from_raw_parts(label.cast::<u8>(), label_len as usize) }
    };

    eprintln!(
        "[DEBUG] {}: {}",
        std::str::from_utf8(label_slice).unwrap_or("???"),
        tag
    );
}

// ============================================================================
// CLI Arguments
// ============================================================================

/// Storage for CLI arguments, initialized once from `main()`.
static CLI_ARGS: OnceLock<CliArgs> = OnceLock::new();

struct CliArgs {
    argc: i64,
    /// Owned copies of the argument strings
    args: Vec<CString>,
}

/// Initialize CLI args (C ABI version for main wrapper).
///
/// This is called from the C `main()` wrapper with (i32, ptr) arguments.
///
/// # Safety
/// - `argc` must match the number of pointers in `argv`
/// - `argv` must be an array of valid null-terminated C strings
/// - Must only be called once (subsequent calls are no-ops)
#[no_mangle]
pub unsafe extern "C" fn tg_init_args_c(argc: i32, argv: *const *const c_char) {
    tg_init_args(i64::from(argc), argv as i64);
}

/// Initialize CLI args (Tungsten ABI version).
///
/// This copies the arguments into owned storage for safe access later.
///
/// # Safety
/// - `argc` must match the number of pointers in `argv`  
/// - `argv` must be a valid pointer to array of C strings (cast to i64)
/// - Must only be called once (subsequent calls are no-ops)
#[no_mangle]
pub unsafe extern "C" fn tg_init_args(argc: i64, argv: i64) {
    // Only initialize once
    if CLI_ARGS.get().is_some() {
        return;
    }

    let argv = argv as *const *const c_char;
    let mut args = Vec::with_capacity(argc as usize);

    for i in 0..argc {
        let arg_ptr = *argv.offset(i as isize);
        if !arg_ptr.is_null() {
            let cstr = CStr::from_ptr(arg_ptr);
            args.push(cstr.to_owned());
        }
    }

    let _ = CLI_ARGS.set(CliArgs {
        argc: args.len() as i64,
        args,
    });
}

/// Get argument count.
#[no_mangle]
pub extern "C" fn tg_argc() -> i64 {
    CLI_ARGS.get().map_or(0, |a| a.argc)
}

/// Get argument at index.
///
/// Returns null if index is out of bounds or args not initialized.
/// The returned string must be freed with `tg_free_string`.
#[no_mangle]
pub extern "C" fn tg_argv(index: i64) -> *mut c_char {
    match CLI_ARGS.get() {
        Some(cli) if index >= 0 && (index as usize) < cli.args.len() => {
            // Return a copy that the caller can free
            cli.args[index as usize].clone().into_raw()
        }
        _ => ptr::null_mut(),
    }
}

// ============================================================================
// Process Control
// ============================================================================

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
