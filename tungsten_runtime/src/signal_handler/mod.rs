//! Stack overflow signal handler for compiled Tungsten programs.
//!
//! Catches SIGSEGV/SIGBUS caused by stack overflow and prints a diagnostic
//! message instead of crashing silently.
//!
//! The implementation is inherently POSIX (`sigaltstack`, `sigaction`, `mmap`),
//! so it is selected by platform:
//!
//! | Target | Module | Behaviour |
//! |---|---|---|
//! | `unix` | [`unix`] | Installs the SIGSEGV/SIGBUS handler (ADR 18.4.26g §5) |
//! | everything else | [`unsupported`] | No-op |
//!
//! The no-op arm exists so `__tungsten_install_signal_handlers` is present on
//! every target rather than only on the ones that can implement it — the crate
//! is a dependency of `tungsten_core`, which ADR 28.7.26a builds for
//! `wasm32-unknown-unknown`, where `libc` exposes no signal API at all.
//! Nothing calls the symbol there (it is emitted only into compiled Tungsten
//! `main()` prologues, and that route needs LLVM), but a target-invariant
//! public API is cheaper for consumers than a `#[cfg]` at every use site.

#[cfg(unix)]
mod unix;

#[cfg(not(unix))]
mod unsupported;

#[cfg(unix)]
pub use unix::__tungsten_install_signal_handlers;

#[cfg(not(unix))]
pub use unsupported::__tungsten_install_signal_handlers;
