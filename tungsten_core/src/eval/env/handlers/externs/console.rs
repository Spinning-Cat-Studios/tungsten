//! Console-output externs the evaluator executes (ADR 28.7.26a §2.2).
//!
//! `println` in `.tg` is not a builtin — it is the extern chain
//! `tg_string_to_cstring` → `tg_println` → `tg_free_string` (see
//! `src/compiler/driver/ffi/process/mod.tg`). The evaluator executes only the
//! externs it has an arm for, and everything else goes **silently `Stuck`**
//! (`docs/repo-memory/adding-tg-ffi-primitives.md` step 3), so without these
//! arms an evaluated program's output does not merely go somewhere else — it
//! never happens. That is the same silent-empty-run failure the capture sink
//! exists to prevent, arriving by a different route, and it is why the sink
//! alone would not have given the playground any output.
//!
//! Every arm calls the *same* `tungsten_core::ffi` entry point compiled code
//! calls, through real C-string pointers rather than evaluator-local handles.
//! That is what keeps the two sides byte-identical for `diff exec`, and it is
//! why an installed capture sink catches evaluated output for free: the bytes
//! pass through `tg_println` either way.
//!
//! No `unsafe` is needed here despite the pointers: the FFI entry points carry
//! safe signatures, and an integer↔pointer cast is safe in Rust — only
//! dereferencing is not, and that happens inside the FFI module.

use crate::eval::{nat_to_term, term_to_nat, StepResult};
use crate::ffi::TgString;
use crate::terms::Term;

/// Execute a console-output extern, or return `None` if `name` is not one.
///
/// `None` (rather than `Stuck`) so the caller can go on to try its other
/// dispatch arms — this module claims only the calls it recognizes.
pub(super) fn step_console_extern(name: &str, values: &[Term]) -> Option<StepResult> {
    match (name, values) {
        ("tg_string_to_cstring", [Term::StringLit(text)]) => Some(cstring_from(text)),
        ("tg_string_len_internal", [Term::StringLit(text)]) => {
            Some(StepResult::Stepped(nat_to_term(text.len())))
        }
        ("tg_free_string", [address]) => Some(free_cstring(address)),
        ("tg_print", [address, len]) => Some(write_console(address, len, crate::ffi::tg_print)),
        ("tg_println", [address, len]) => Some(write_console(address, len, crate::ffi::tg_println)),
        ("tg_eprintln", [address, len]) => {
            Some(write_console(address, len, crate::ffi::tg_eprintln))
        }
        _ => None,
    }
}

/// Allocate a null-terminated C copy of `text` and step to its address.
///
/// The caller owns it and must pass it to `tg_free_string`, exactly as the
/// `.tg` wrapper does — this is the real allocation, not a handle into a
/// side table, so a later `tg_println` reads the same bytes native would.
fn cstring_from(text: &str) -> StepResult {
    let borrowed = TgString {
        ptr: text.as_ptr().cast::<std::ffi::c_char>(),
        len: text.len() as u64,
    };
    let owned = crate::ffi::tg_string_to_cstring(borrowed);
    StepResult::Stepped(nat_to_term(owned as usize))
}

/// Release a C string previously produced by [`cstring_from`].
///
/// A non-`Nat` operand stays `Stuck` rather than being coerced: freeing a
/// misinterpreted address is exactly the mistake worth refusing to guess at.
fn free_cstring(address: &Term) -> StepResult {
    match term_to_nat(address) {
        Some(address) => {
            crate::ffi::tg_free_string(address as *mut std::ffi::c_char);
            StepResult::Stepped(Term::Unit)
        }
        None => StepResult::Stuck,
    }
}

/// Marshal a `(address, len)` pair and hand it to a console FFI entry point.
///
/// `write` is `tg_print`, `tg_println`, or `tg_eprintln` — whichever the call
/// named. When a capture sink is installed, that is where the bytes land.
fn write_console(
    address: &Term,
    len: &Term,
    write: extern "C" fn(*const std::ffi::c_char, u64),
) -> StepResult {
    match (term_to_nat(address), term_to_nat(len)) {
        (Some(address), Some(len)) => {
            write(address as *const std::ffi::c_char, len as u64);
            StepResult::Stepped(Term::Unit)
        }
        _ => StepResult::Stuck,
    }
}

// Tests: console_tests.rs — kept beside this module so its coverage of
// the capture-sink interaction can grow without crowding the dispatch table.
#[cfg(test)]
#[path = "console_tests.rs"]
mod tests;
