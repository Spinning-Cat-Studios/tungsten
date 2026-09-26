//! Process-global registry mapping a comparator symbol to the concrete `Type`
//! it compares (ADR 29.6.26f §T11.2).
//!
//! `elab_compare` cannot easily thread the operand `Type` through the
//! elaboration→driver boundary, and a composite comparator's *name* is not
//! reversible to its `Type`. So the elaborator records `(symbol, Type)` here as
//! it emits each `__compare` call, and the synthesis pass resolves the symbols
//! it discovers in the codegen units / eval defs back to their types.
//!
//! This mirrors the existing global driver config (`diagnostics::set_max_errors`).
//! It is prototype-grade: a typed side-channel on `ElabOutput` is the eventual
//! clean form. Callers should `clear()` at the start of a compilation so stale
//! entries from a prior in-process invocation cannot leak.

use std::collections::HashMap;
use std::sync::Mutex;

use tungsten_core::Type;

static REQUESTS: Mutex<Option<HashMap<String, Type>>> = Mutex::new(None);

/// Record that `symbol` compares values of `ty`.
pub fn register(symbol: String, ty: Type) {
    let mut guard = REQUESTS
        .lock()
        .expect("comparator request registry poisoned");
    guard.get_or_insert_with(HashMap::new).insert(symbol, ty);
}

/// Resolve a comparator symbol to the `Type` it compares, if recorded.
#[must_use]
pub fn lookup(symbol: &str) -> Option<Type> {
    REQUESTS
        .lock()
        .expect("comparator request registry poisoned")
        .as_ref()
        .and_then(|m| m.get(symbol).cloned())
}

/// Clear all recorded requests (call at the start of a compilation).
pub fn clear() {
    *REQUESTS
        .lock()
        .expect("comparator request registry poisoned") = None;
}

/// Serializes every test that touches this registry — **directly** via
/// `register`/`clear`, or **indirectly** by elaborating a project (the driver
/// clears the registry at the start of each compilation).
///
/// It lives here, beside the state it protects, rather than in any one test
/// module. A `static Mutex` per test file does not serialize anything: the
/// modules compile into one multi-threaded test binary and race, and the
/// default `--test-threads` schedule can hide it for a long time (ADR
/// 28.7.26a). That is exactly how it failed — `discover/tests.rs` held its own
/// guard, and the first test elsewhere to elaborate a project wiped the
/// registry out from under it.
#[cfg(test)]
pub(crate) static TEST_EXCLUSIVE: Mutex<()> = Mutex::new(());
