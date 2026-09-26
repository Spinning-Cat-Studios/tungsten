//! Tests for driver FFI functions, split by FFI surface.
//!
//! Was a single `tests.rs` until it reached 367 of the 400-line cap with four
//! more tests due (ADR 7.8.26b `/check-adr`). The seam is the one `io/` itself
//! already uses, so a reader looking for a filesystem test looks in the file
//! named for it.

mod diagnostics;
mod filesystem;
mod process;
mod strings;
