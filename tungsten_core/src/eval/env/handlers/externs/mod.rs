//! Executing `extern "C"` calls during evaluation.
//!
//! Three concerns, one per file, because they answer different questions:
//!
//! | Module | Question |
//! |---|---|
//! | [`call`] | Dispatch: what happens when an `ExternCall` is stepped? |
//! | [`console`] | The console-output arms (ADR 28.7.26a §2.2) |
//! | [`arena`] | The type-arena arms (ADR 7.8.26c) |
//! | [`builder`] | The `StringBuilder` arms (ADR 14.9.26a) |
//! | [`registry`] | Which externs are executable at all — the tooling surface |
//!
//! The property tying them together: the evaluator executes only the externs it
//! has an arm for, and **every other `ExternCall` goes silently `Stuck`** — no
//! error, no warning, no output. [`registry`] exists so that fact is
//! inspectable (`tungsten info eval externs`, `tungsten doctor check
//! extern-coverage`) rather than discoverable only by reading match arms.
//!
//! Adding an extern means editing **two** of these: an arm in one of the
//! dispatch modules the table above lists, and an entry in [`registry`].
//! `registry`'s tests drive every entry through the real dispatchers, so the
//! pair cannot silently diverge in the direction that would make the
//! diagnostics lie. The table is the list; a new dispatch module joins it.

pub(super) mod arena;
pub(super) mod builder;
pub(super) mod call;
pub(super) mod console;
pub mod registry;
