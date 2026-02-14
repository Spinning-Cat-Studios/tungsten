//! Evaluator FFI - Bridge from self-hosted driver to Rust evaluator
//!
//! This module provides FFI functions for the self-hosted driver to execute
//! Tungsten programs using the bootstrap evaluator. The key insight is that
//! elaborated terms are already stored in the global arena, so we just need
//! to build an `EvalEnv` and call `eval_with_env`.
//!
//! ## Usage Pattern (from Tungsten)
//!
//! ```tungsten
//! // After elaboration, we have a list of (name, term_handle) pairs
//! let env_handle = tg_eval_env_new();
//! for_each(defs, |def| {
//!     tg_eval_env_add(env_handle, def.name, def.term);
//! });
//! let result = tg_eval_run(main_term, env_handle);
//! let output = tg_eval_display(result);
//! tg_eval_env_free(env_handle);
//! ```

use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::ptr;

use crate::eval::{eval_with_env, eval_with_env_and_limit, EvalEnv};

use super::super::{with_arena_ref, TermHandle, INVALID_HANDLE};
use super::set_driver_error;

// ============================================================================
// EvalEnv Handle
// ============================================================================

/// Opaque handle to an evaluation environment
pub type EvalEnvHandle = u64;

/// Invalid environment handle sentinel
pub const INVALID_ENV_HANDLE: EvalEnvHandle = u64::MAX;

// Store environments in a thread-local vec (similar to arena pattern)
thread_local! {
    static EVAL_ENVS: std::cell::RefCell<Vec<Option<HashMap<String, crate::Term>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

// ============================================================================
// Environment Creation and Management
// ============================================================================

/// Create a new empty evaluation environment.
/// Returns an environment handle, or `INVALID_ENV_HANDLE` on error.
#[no_mangle]
pub extern "C" fn tg_eval_env_new() -> EvalEnvHandle {
    EVAL_ENVS.with(|envs| {
        let mut envs = envs.borrow_mut();
        let handle = envs.len() as EvalEnvHandle;
        envs.push(Some(HashMap::new()));
        handle
    })
}

/// Add a definition to an evaluation environment.
///
/// # Arguments
/// * `env_handle` - Handle to the environment (from `tg_eval_env_new`)
/// * `name` - Null-terminated C string with the definition name
/// * `term_handle` - Handle to the term in the arena
///
/// # Returns
/// * 1 on success, 0 on error (invalid handles)
#[no_mangle]
pub unsafe extern "C" fn tg_eval_env_add(
    env_handle: EvalEnvHandle,
    name: *const c_char,
    term_handle: TermHandle,
) -> i32 {
    if name.is_null() {
        set_driver_error("tg_eval_env_add: name is null");
        return 0;
    }

    let name_str = if let Ok(s) = CStr::from_ptr(name).to_str() {
        s.to_string()
    } else {
        set_driver_error("tg_eval_env_add: invalid UTF-8 in name");
        return 0;
    };

    // Get the term from the arena
    let term = with_arena_ref!(|arena| arena.get_term(term_handle).cloned());

    let term = if let Some(t) = term {
        t
    } else {
        set_driver_error(format!(
            "tg_eval_env_add: invalid term handle {term_handle}"
        ));
        return 0;
    };

    // Add to environment
    EVAL_ENVS.with(|envs| {
        let mut envs = envs.borrow_mut();
        if let Some(Some(map)) = envs.get_mut(env_handle as usize) {
            map.insert(name_str, term);
            1
        } else {
            set_driver_error(format!("tg_eval_env_add: invalid env handle {env_handle}"));
            0
        }
    })
}

/// Free an evaluation environment.
/// After this call, the handle is invalid and must not be used.
#[no_mangle]
pub extern "C" fn tg_eval_env_free(env_handle: EvalEnvHandle) {
    EVAL_ENVS.with(|envs| {
        let mut envs = envs.borrow_mut();
        if let Some(slot) = envs.get_mut(env_handle as usize) {
            *slot = None;
        }
    });
}

// ============================================================================
// Evaluation
// ============================================================================

/// Evaluate a term using an environment and return the result.
///
/// # Arguments
/// * `term_handle` - Handle to the term to evaluate (typically main's body)
/// * `env_handle` - Handle to the environment with global definitions
///
/// # Returns
/// * Handle to the evaluated value in the arena, or `INVALID_HANDLE` on error
#[no_mangle]
pub extern "C" fn tg_eval_run(term_handle: TermHandle, env_handle: EvalEnvHandle) -> TermHandle {
    tg_eval_run_with_limit(term_handle, env_handle, 0)
}

/// Evaluate a term with a step limit.
///
/// # Arguments
/// * `term_handle` - Handle to the term to evaluate
/// * `env_handle` - Handle to the environment with global definitions
/// * `limit` - Maximum evaluation steps (0 = no limit)
///
/// # Returns
/// * Handle to the evaluated value, or `INVALID_HANDLE` on error/timeout
#[no_mangle]
pub extern "C" fn tg_eval_run_with_limit(
    term_handle: TermHandle,
    env_handle: EvalEnvHandle,
    limit: u64,
) -> TermHandle {
    // Get the term from the arena
    let term = with_arena_ref!(|arena| arena.get_term(term_handle).cloned());

    let term = if let Some(t) = term {
        t
    } else {
        set_driver_error(format!("tg_eval_run: invalid term handle {term_handle}"));
        return INVALID_HANDLE;
    };

    // Get the environment
    let globals = EVAL_ENVS.with(|envs| {
        let envs = envs.borrow();
        envs.get(env_handle as usize)
            .and_then(std::clone::Clone::clone)
    });

    let globals = if let Some(g) = globals {
        g
    } else {
        set_driver_error(format!("tg_eval_run: invalid env handle {env_handle}"));
        return INVALID_HANDLE;
    };

    // Build EvalEnv and evaluate
    let env = EvalEnv::new(globals);

    let result = if limit == 0 {
        eval_with_env(&term, &env)
    } else {
        if let Some(t) = eval_with_env_and_limit(&term, &env, limit as usize) {
            t
        } else {
            set_driver_error(format!("tg_eval_run: evaluation exceeded {limit} steps"));
            return INVALID_HANDLE;
        }
    };

    // Store result in arena and return handle
    crate::ffi::with_arena!(|arena| arena.alloc_term(result))
}

// ============================================================================
// Result Display
// ============================================================================

/// Get a string representation of an evaluated term.
///
/// # Arguments
/// * `term_handle` - Handle to the term to display
///
/// # Returns
/// * Null-terminated C string (must be freed with `tg_free_string`), or null on error
#[no_mangle]
pub extern "C" fn tg_eval_display(term_handle: TermHandle) -> *mut c_char {
    let term = with_arena_ref!(|arena| arena.get_term(term_handle).cloned());

    let term = if let Some(t) = term {
        t
    } else {
        set_driver_error(format!(
            "tg_eval_display: invalid term handle {term_handle}"
        ));
        return ptr::null_mut();
    };

    // Use Debug formatting for now (Term implements Debug)
    // In production, we'd use a proper pretty-printer
    let display = format!("{term:?}");

    if let Ok(s) = CString::new(display) {
        s.into_raw()
    } else {
        set_driver_error("tg_eval_display: result contains null byte");
        ptr::null_mut()
    }
}

/// Get a simplified string representation of an evaluated term.
/// This is more user-friendly than `tg_eval_display` for simple values.
///
/// # Returns
/// * For Nat values: the numeric value as a string
/// * For Unit: "()"
/// * For other values: the debug representation
#[no_mangle]
pub extern "C" fn tg_eval_display_value(term_handle: TermHandle) -> *mut c_char {
    use crate::Term;

    let term = with_arena_ref!(|arena| arena.get_term(term_handle).cloned());

    let term = if let Some(t) = term {
        t
    } else {
        set_driver_error(format!(
            "tg_eval_display_value: invalid term handle {term_handle}"
        ));
        return ptr::null_mut();
    };

    // Format based on term type
    let display = match &term {
        Term::Unit => "()".to_string(),
        Term::Zero => "0".to_string(),
        Term::Succ(_) => {
            // Count successors to get natural number value
            let mut n = 0usize;
            let mut current = &term;
            while let Term::Succ(inner) = current {
                n += 1;
                current = inner.as_ref();
            }
            if matches!(current, Term::Zero) {
                n.to_string()
            } else {
                format!("{term:?}")
            }
        }
        Term::StringLit(s) => format!("\"{s}\""),
        // For complex values, use debug format
        _ => format!("{term:?}"),
    };

    if let Ok(s) = CString::new(display) {
        s.into_raw()
    } else {
        set_driver_error("tg_eval_display_value: result contains null byte");
        ptr::null_mut()
    }
}
