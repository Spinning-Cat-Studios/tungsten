//! Safe entry points for the three arena externs whose real symbols are
//! `unsafe` or return a raw fat pointer (ADR 7.8.26c D5).
//!
//! The evaluator's arena arms ([`crate::eval`]'s `externs::arena`) are a
//! marshal → call → marshal sandwich over the *real* `tg_*` symbol, so the
//! evaluated and compiled paths cannot drift. Twelve of the fifteen arms can
//! say that directly — their symbols carry safe signatures. Three cannot:
//! `tg_type_mu` and `tg_string_to_cstr` are `unsafe extern "C"`, and
//! `tg_cstring_to_string` hands back a [`TgString`] whose bytes only an
//! `unsafe` read can reach.
//!
//! The workspace denies `unsafe_code` and overrides it **for FFI modules
//! only** (`Cargo.toml`), which is why these three wrappers live here rather
//! than as an `#[allow]` in the evaluator. Each is a call and nothing else: no
//! arm re-derives what the symbol does, which is the property D5 exists to
//! protect.
//!
//! The `StringBuilder` pair (ADR 14.9.26a) joins them for the same reason:
//! `tg_string_builder_to_string` hands back a [`TgString`] whose bytes only an
//! `unsafe` read can reach, and `tg_string_builder_push_str` takes one.

use std::os::raw::c_char;

use super::{TgString, TypeHandle};

/// Construct `μ<name>. <body>` where `name_address` is a C string, as
/// `tg_type_mu` itself takes it.
///
/// A null (0) address yields `INVALID_HANDLE`, which is the symbol's own
/// contract for it — not a case this wrapper decides.
pub(crate) fn mu_type_from_cstr(name_address: usize, body: TypeHandle) -> TypeHandle {
    // SAFETY: the address came from `tg_string_to_cstr` or
    // `tg_type_get_mu_var`, both of which leak a null-terminated `CString`
    // that outlives the run; `tg_type_mu` null-checks 0 itself.
    unsafe { super::types::constructors::tg_type_mu(name_address as *const c_char, body) }
}

/// Leak a null-terminated C copy of `text` and return its address, exactly as
/// the `.tg` call `tg_string_to_cstr(s)` does.
///
/// Returns 0 (null) when `text` contains an interior NUL, matching the
/// symbol's behaviour rather than papering over it.
pub(crate) fn cstr_address_of(text: &str) -> usize {
    // SAFETY: `text` is a live Rust `str`, so ptr/len describe exactly
    // `text.len()` initialized UTF-8 bytes for the duration of the call.
    let address =
        unsafe { super::tg_string_to_cstr(text.as_ptr().cast::<c_char>(), text.len() as u64) };
    address as usize
}

/// Read the C string at `address` back into an owned Rust `String`, through
/// the same `tg_cstring_to_string` the compiled path calls.
///
/// A null (0) address yields the empty string — the symbol's own answer for
/// it, and the one `cstring_to_string` in `.tg` observes.
pub(crate) fn string_at_cstr_address(address: usize) -> String {
    let copied = super::tg_cstring_to_string(address as *const c_char);
    tg_string_text(copied)
}

/// Consume the `StringBuilder` at `handle` and copy its text out, through the
/// same `tg_string_builder_to_string` the compiled path calls (ADR 14.9.26a).
///
/// The builder is dead afterwards, exactly as it is natively; the buffer the
/// symbol hands out is copied into the `String` the evaluator holds and then
/// leaked, like every other C buffer the evaluator receives (see
/// [`string_at_cstr_address`]). A null-handle or consumed-handle abort is the
/// symbol's own, not this wrapper's.
pub(crate) fn string_builder_take_text(handle: u64) -> String {
    tg_string_text(super::tg_string_builder_to_string(handle))
}

/// Append `text` to the `StringBuilder` at `handle`, through the same
/// `tg_string_builder_push_str` the compiled path calls (ADR 14.9.26a).
///
/// Here rather than in the evaluator arm because the `TgString` it borrows
/// `text` as is an FFI type, and building one is the marshalling step this
/// module exists to keep on the FFI side of the `unsafe_code` boundary.
pub(crate) fn string_builder_push_text(handle: u64, text: &str) -> u64 {
    let borrowed = TgString {
        ptr: text.as_ptr().cast::<c_char>(),
        len: text.len() as u64,
    };
    super::tg_string_builder_push_str(handle, borrowed)
}

/// Copy a [`TgString`]'s bytes into an owned `String`.
///
/// Lossy rather than fallible: every producer on this path is a `CString`
/// built from valid UTF-8, so invalid bytes are unreachable, and returning
/// `Option` here would push a `None` arm into the evaluator that no input can
/// reach and no test can kill.
/// The null check is the *only* guard, deliberately: a `|| s.len == 0`
/// companion reads as defensive and is unreachable, because
/// `tg_cstring_to_string` returns a null pointer in exactly the cases where
/// the length would be zero. The mutation sweep is what proved it — flipping
/// that `||` to `&&` changed no behaviour on any input, which is the signature
/// of a clause carrying nothing.
pub(in crate::ffi) fn tg_string_text(s: TgString) -> String {
    if s.ptr.is_null() {
        return String::new();
    }
    // SAFETY: `tg_cstring_to_string` returns a freshly malloc'd buffer of
    // exactly `len` bytes, or a null pointer — ruled out above. `c_char` has
    // alignment 1, so a zero `len` here would still be a sound empty slice.
    let bytes = unsafe { std::slice::from_raw_parts(s.ptr.cast::<u8>(), s.len as usize) };
    String::from_utf8_lossy(bytes).into_owned()
}

// Tests: tests.rs — kept beside this module so the round-trip coverage of the
// three `unsafe` call sites can grow without crowding the wrappers themselves.
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
