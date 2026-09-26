//! `StringBuilder` externs the evaluator executes (ADR 14.9.26a).
//!
//! `src/compiler/driver/ffi/builder/mod.tg` declares six `tg_string_builder_*`
//! externs and wraps them in a single-constructor ADT. Without these arms every
//! call goes silently `Stuck`, and `test_string_builder.tg` would report `ok`
//! while asserting nothing — the third time that trap would have caught a
//! suite (ADR 6.8.26b, 20.8.26d, 19.8.26c). So evaluator support is part of
//! the definition of done, not an afterthought.
//!
//! ## Every arm calls the real symbol
//!
//! A handle is the address of a header the runtime owns, marshalled as `Nat`,
//! so the evaluated and compiled paths share one heap and one growth policy:
//! no arm re-derives what a `tg_string_builder_*` function does. The two
//! symbols that take or return a `TgString` are reached through the safe
//! wrappers in `crate::ffi::evaluator_bridges`, because the workspace allows
//! `unsafe_code` in FFI modules only.
//!
//! ## Arity follows the `.tg` declaration
//!
//! `tg_string_builder_push_str(sb: Nat, s: String)` arrives as a `Nat` and
//! **one** `StringLit` — the Rust `(u64, TgString)` is the C ABI, not the Core
//! arity — exactly as `tg_string_to_cstr` does (ADR 7.8.26c).
//!
//! ## Aborts
//!
//! A null or consumed handle aborts the *process*, natively and here alike:
//! the runtime's own poison check runs before any arm reads anything, and the
//! evaluator does not second-guess it. That is the parity `diff exec` asserts.

use crate::eval::{nat_to_term, term_to_nat, StepResult};
use crate::ffi::{
    string_builder_push_text, string_builder_take_text, tg_string_builder_len,
    tg_string_builder_new, tg_string_builder_push_char, tg_string_builder_with_capacity,
};
use crate::terms::Term;

/// Execute a `StringBuilder` extern, or return `None` if `name` is not one.
///
/// `None` (rather than `Stuck`) so the caller goes on to try its other
/// dispatch arms — this module claims only the calls it recognizes, and a
/// wrong-arity call is *not* recognized.
pub(super) fn step_builder_extern(name: &str, values: &[Term]) -> Option<StepResult> {
    match (name, values) {
        ("tg_string_builder_new", []) => Some(stepped_handle(tg_string_builder_new())),
        ("tg_string_builder_with_capacity", [capacity]) => Some(step_handle_in_handle_out(
            capacity,
            tg_string_builder_with_capacity,
        )),
        ("tg_string_builder_push_str", [handle, Term::StringLit(text)]) => {
            Some(step_push_str(handle, text))
        }
        ("tg_string_builder_push_char", [handle, code]) => Some(step_binary_nat_extern(
            handle,
            code,
            tg_string_builder_push_char,
        )),
        ("tg_string_builder_len", [handle]) => {
            Some(step_handle_in_handle_out(handle, tg_string_builder_len))
        }
        ("tg_string_builder_to_string", [handle]) => Some(step_to_string(handle)),
        _ => None,
    }
}

/// Marshal one `Nat` operand and call a `u64 -> u64` symbol.
///
/// Serves `with_capacity` (a count in, a handle out) and `len` (a handle in,
/// a count out) alike — on the `.tg` side both are `Nat -> Nat`.
///
/// A non-`Nat` operand stays `Stuck` rather than being coerced: a
/// misinterpreted handle would be read as a header address.
fn step_handle_in_handle_out(operand: &Term, symbol: extern "C" fn(u64) -> u64) -> StepResult {
    match term_to_nat(operand) {
        Some(operand) => stepped_handle(symbol(operand as u64)),
        None => StepResult::Stuck,
    }
}

/// Marshal two `Nat` operands and call a `(u64, u64) -> u64` symbol.
fn step_binary_nat_extern(
    left: &Term,
    right: &Term,
    symbol: extern "C" fn(u64, u64) -> u64,
) -> StepResult {
    match (term_to_nat(left), term_to_nat(right)) {
        (Some(left), Some(right)) => stepped_handle(symbol(left as u64, right as u64)),
        _ => StepResult::Stuck,
    }
}

/// Append `text` to the builder at `handle`; steps to the same handle.
fn step_push_str(handle: &Term, text: &str) -> StepResult {
    match term_to_nat(handle) {
        Some(handle) => stepped_handle(string_builder_push_text(handle as u64, text)),
        None => StepResult::Stuck,
    }
}

/// Consume the builder at `handle`; steps to its text as a `StringLit`.
fn step_to_string(handle: &Term) -> StepResult {
    match term_to_nat(handle) {
        Some(handle) => {
            StepResult::Stepped(Term::StringLit(string_builder_take_text(handle as u64)))
        }
        None => StepResult::Stuck,
    }
}

/// Step to a handle or a byte count — both `Nat` on the `.tg` side.
///
/// Routed through `nat_to_term`, and so through `Term::nat_smart`: a heap
/// address is far past the point where a `Succ` chain would wedge (ADR
/// 21.7.26e).
fn stepped_handle(value: u64) -> StepResult {
    StepResult::Stepped(nat_to_term(value as usize))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
