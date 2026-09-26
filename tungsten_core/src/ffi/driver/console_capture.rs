//! In-process capture sink for the console FFI (ADR 28.7.26a §2.2).
//!
//! `tg_print`/`tg_println`/`tg_eprintln` normally write straight to the
//! process's streams. On `wasm32-unknown-unknown` there are no such streams, so
//! those writes are discarded and a program's output vanishes silently — the
//! worst failure mode for a playground, because it looks like a working run
//! that printed nothing. This module is the sink that catches them instead.
//!
//! Existing in-process consumers were not an option: `diff exec` compares
//! evaluator and native output by capturing a *subprocess*'s stdout, which
//! needs a process to capture.
//!
//! Three properties this is built around:
//!
//! - **Opt-in, and byte-identical when off.** With no sink installed, every
//!   write takes exactly the path it took before this module existed. The only
//!   cost is one relaxed atomic load per call, which is why [`is_active`] is a
//!   plain flag rather than a lock acquisition — `tg_print` is on compiled
//!   code's hot path.
//! - **Separate stdout and stderr buffers.** Merging at the source destroys a
//!   distinction that cannot be recovered afterwards; a consumer that wants one
//!   pane can merge at the presentation layer, which is its call to make.
//! - **One consumer at a time.** The sink is process-global mutable state, the
//!   same class as `tg_init_args`. [`install`] takes it, [`take`] releases it,
//!   and a second [`install`] while one is live is a caller bug — it is
//!   reported rather than silently interleaving two runs' output.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Whether a sink is installed.
///
/// Read on every console write, so it is deliberately a lone relaxed atomic:
/// the uninstalled path must not pay for a lock. `Relaxed` is sufficient
/// because [`BUFFERS`]'s own mutex orders every access to the data itself; this
/// flag only decides which branch to take.
static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// The installed sink's buffers, or `None` when nothing is installed.
static BUFFERS: Mutex<Option<CapturedOutput>> = Mutex::new(None);

/// Bytes a capture run collected, per stream.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CapturedOutput {
    /// Everything written via `tg_print` / `tg_println`.
    pub stdout: Vec<u8>,
    /// Everything written via `tg_eprintln`.
    pub stderr: Vec<u8>,
}

/// Why an [`install`] call could not take the sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallError {
    /// A sink is already installed. Call [`take`] before installing another —
    /// two live captures would interleave unrelated runs' output.
    AlreadyInstalled,
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyInstalled => {
                write!(f, "a console capture sink is already installed")
            }
        }
    }
}

impl std::error::Error for InstallError {}

/// Exclusive test access to the process-global sink.
///
/// The sink is one resource, so it needs **one** lock. Two test modules in this
/// crate exercise it — `console_capture_tests.rs` (the sink itself) and
/// `eval/env/handlers/extern_console_tests.rs` (the evaluator's console
/// externs) — and they compile into the same, multi-threaded test binary. When
/// each declared its own `Mutex<()>`, they did not serialize against each
/// other: one test's setup would [`take`] the sink another had just installed,
/// and the victim then failed on `take().expect("a sink was installed")`. That
/// reproduced roughly one run in eight at `--test-threads=16` while the default
/// schedule happened to hide it.
///
/// Mirrors the `INTEGRATION_LOCK` idiom in `bootstrap/src/sidecar/process/`:
/// one `pub(crate)` lock, declared beside the resource rather than beside a
/// caller, so a third test module cannot reintroduce the split by declaring its
/// own.
#[cfg(test)]
pub(crate) mod test_exclusive {
    use std::sync::{Mutex, MutexGuard, PoisonError};

    /// The single lock guarding the sink across every test module in this crate.
    static SINK_LOCK: Mutex<()> = Mutex::new(());

    /// Exclusive use of the sink, released on drop.
    ///
    /// Clears the sink on both acquire and release, so a test that panics
    /// mid-capture cannot strand an installed sink and cascade into the next.
    pub(crate) struct SinkGuard(#[allow(dead_code)] MutexGuard<'static, ()>);

    impl Drop for SinkGuard {
        fn drop(&mut self) {
            let _ = super::take();
        }
    }

    /// Claim exclusive use of the process-global capture sink for this test.
    ///
    /// Hold the returned guard for the whole test: dropping it releases the
    /// lock, so binding it to `_` rather than `_guard` would release it
    /// immediately and reopen the race this exists to close.
    pub(crate) fn exclusive_sink() -> SinkGuard {
        let guard = SINK_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = super::take();
        SinkGuard(guard)
    }
}

/// Whether console writes are currently being captured.
///
/// This is the check every console FFI entry point makes first; it must stay
/// cheap enough to sit on compiled code's output path.
pub fn is_active() -> bool {
    CAPTURE_ACTIVE.load(Ordering::Relaxed)
}

/// Install a fresh capture sink, routing subsequent console writes into it.
///
/// # Errors
///
/// Returns [`InstallError::AlreadyInstalled`] if a sink is already installed.
pub fn install() -> Result<(), InstallError> {
    let mut slot = lock_buffers();
    if slot.is_some() {
        return Err(InstallError::AlreadyInstalled);
    }
    *slot = Some(CapturedOutput::default());
    // Ordered after the buffers are in place: a writer that sees the flag set
    // must find something to write into.
    CAPTURE_ACTIVE.store(true, Ordering::Release);
    Ok(())
}

/// Uninstall the sink and return what it captured, or `None` if none was
/// installed.
///
/// Subsequent console writes go back to the process streams.
pub fn take() -> Option<CapturedOutput> {
    CAPTURE_ACTIVE.store(false, Ordering::Release);
    lock_buffers().take()
}

/// Append `parts` to the captured stdout buffer.
///
/// Returns `true` when the bytes were captured, and `false` when no sink is
/// installed — in which case the caller must fall through to its normal stream
/// write. Callers check [`is_active`] first; this re-checks under the lock, so
/// a sink removed in between is not written into.
fn capture_stdout(parts: &[&[u8]]) -> bool {
    append(parts, |captured| &mut captured.stdout)
}

/// Append `parts` to the captured stderr buffer.
///
/// Same contract as [`capture_stdout`].
fn capture_stderr(parts: &[&[u8]]) -> bool {
    append(parts, |captured| &mut captured.stderr)
}

/// Append `parts` to whichever buffer `select` picks, reporting whether a sink
/// was there to append to.
///
/// All parts land under one lock acquisition, so a `tg_println`'s text and its
/// newline cannot be split by a concurrent write.
fn append(parts: &[&[u8]], select: impl FnOnce(&mut CapturedOutput) -> &mut Vec<u8>) -> bool {
    let mut slot = lock_buffers();
    match slot.as_mut() {
        Some(captured) => {
            let buffer = select(captured);
            for part in parts {
                buffer.extend_from_slice(part);
            }
            true
        }
        None => false,
    }
}

/// Lock [`BUFFERS`], recovering the guard if a previous holder panicked.
///
/// A poisoned sink is not a reason to abort a compiled program's output path:
/// the buffers are plain byte vectors with no invariant a panic could break,
/// so the worst a recovered guard carries is a partial line.
fn lock_buffers() -> std::sync::MutexGuard<'static, Option<CapturedOutput>> {
    BUFFERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Write `parts` to stdout when no sink is installed, or capture them when one
/// is. The shared tail of `tg_print` and `tg_println`.
///
/// `parts` is a slice rather than one buffer so the uninstalled path reproduces
/// the original call sequence exactly — `tg_println` issued a `write_all` per
/// part and a single trailing `flush`, and joining them here would allocate on
/// the path this module promises not to disturb.
pub(super) fn write_stdout(parts: &[&[u8]]) {
    if is_active() && capture_stdout(parts) {
        return;
    }
    let mut stdout = std::io::stdout();
    for part in parts {
        let _ = stdout.write_all(part);
    }
    let _ = stdout.flush();
}

/// Write `parts` to stderr when no sink is installed, or capture them when one
/// is. The shared tail of `tg_eprintln`.
///
/// Same `parts` rationale as [`write_stdout`].
pub(super) fn write_stderr(parts: &[&[u8]]) {
    if is_active() && capture_stderr(parts) {
        return;
    }
    let mut stderr = std::io::stderr();
    for part in parts {
        let _ = stderr.write_all(part);
    }
    let _ = stderr.flush();
}

// Tests: console_capture_tests.rs — kept beside this module rather than inline
// so the sink stays inside the .rs file-size limit as its test surface grows.
#[cfg(test)]
#[path = "console_capture_tests.rs"]
mod tests;
