//! No-op stack-overflow handler for targets without POSIX signals.
//!
//! Selected on every non-`unix` target — in practice `wasm32-unknown-unknown`,
//! which ADR 28.7.26a builds for. There is no signal to catch there: a wasm
//! stack exhaustion traps and unwinds the instance, which the embedder
//! observes directly, so there is nothing this handler could add.
//!
//! See [`super`] for why the symbol exists at all rather than being
//! `#[cfg]`-ed away.

/// Install stack-overflow signal handlers — a no-op on this target.
///
/// Mirrors the `unix` arm's signature so compiled-code prologues and any
/// direct caller link identically on every target.
#[no_mangle]
pub extern "C" fn __tungsten_install_signal_handlers() {}

#[cfg(test)]
mod tests {
    use super::__tungsten_install_signal_handlers;

    /// The no-op arm must be callable and must not panic — it is invoked from
    /// a `main()` prologue, where a panic would be a hard startup failure.
    #[test]
    fn install_is_a_noop_that_returns() {
        __tungsten_install_signal_handlers();
    }
}
