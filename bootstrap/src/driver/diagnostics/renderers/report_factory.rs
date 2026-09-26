//! The one door every ariadne report is built through (ADR 24.9.26b).
//!
//! The bootstrap's spans are **byte** offsets — the lexer, the cache and the
//! conformance tools all speak bytes — but ariadne 0.4's `Report` reads spans
//! as **character** offsets unless told otherwise. Left at the default, every
//! multi-byte character before a span (an em-dash in a leading comment is
//! three bytes, one character) moves the label and its `file:line:col` heading
//! one column right per extra byte, on every later line of the file.
//!
//! The spans are right; only the renderer's reading of them was wrong, so the
//! fix is configuration, not conversion. In byte mode ariadne still prints the
//! heading's column in characters, 1-based — what an editor's cursor shows.
//!
//! A test in `tests.rs` scans `bootstrap/src/` and fails on any direct
//! `Report` builder call outside this file, so a new site cannot silently
//! regress to the character default.

use ariadne::{Config, IndexType, Report, ReportBuilder, ReportKind};
use std::ops::Range;

/// The configuration every diagnostic report renders with: byte-indexed spans.
pub(super) fn report_config() -> Config {
    Config::default().with_index_type(IndexType::Byte)
}

/// Start a report of `kind` in `file` at byte `offset`, byte-indexed.
pub(super) fn build_report<'a>(
    kind: ReportKind<'a>,
    file: &'a str,
    offset: usize,
) -> ReportBuilder<'a, (&'a str, Range<usize>)> {
    Report::build(kind, file, offset).with_config(report_config())
}
