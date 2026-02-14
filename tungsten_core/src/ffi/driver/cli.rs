//! CLI argument FFI functions

use std::ffi::{c_char, CStr, CString};
use std::ptr;
use std::sync::OnceLock;

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
