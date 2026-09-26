//! Every step of `info error-sites` is a pure function over strings, so the
//! whole report is asserted from in-memory fixtures — no filesystem, no
//! elaboration. The exceptions — the directory walk and `run` itself — are
//! exercised against the real source tree in [`scanning`].
//!
//! This file holds the shared fixtures and the parsing half (the two source
//! tables and query resolution); [`scanning`] holds site discovery, the
//! report, and the I/O wrapper.

mod scanning;

use super::*;

pub(super) const CODES: &str = r#"
error_codes! {
    ElabErrorKind::UndefinedVariable(_) => "E0001",
    ElabErrorKind::ExpectedFunction(_) => "E0013",
    ElabErrorKind::ModuleNotFound { .. } => "E0005",
}
"#;

pub(super) const CTORS: &str = r#"
impl ElabError {
    /// Create an "expected function" error.
    pub fn expected_function(span: Span, found: Type) -> Self {
        Self::new(span, ElabErrorKind::ExpectedFunction(found))
    }

    pub fn module_not_found(span: Span, m: String) -> Self {
        Self::new(
            span,
            ElabErrorKind::ModuleNotFound { module: m, suggestion: None },
        )
    }
}
"#;

// ─────────────────────────────────────────────────────────────────────
// parse_kind_codes
// ─────────────────────────────────────────────────────────────────────

#[test]
fn the_table_parses_one_pair_per_arm() {
    let pairs = parse_kind_codes(CODES);
    assert_eq!(pairs.len(), 3, "{pairs:?}");
    assert!(pairs.contains(&("ExpectedFunction".into(), "E0013".into())));
    assert!(
        pairs.contains(&("ModuleNotFound".into(), "E0005".into())),
        "a struct-variant arm must parse too: {pairs:?}"
    );
}

/// A reformat that broke the one-arm-one-line contract must yield nothing
/// rather than a partial table that silently loses codes.
#[test]
fn a_table_that_does_not_match_the_shape_parses_empty() {
    assert!(parse_kind_codes("fn code(&self) -> &str { \"E0001\" }").is_empty());
}

// ─────────────────────────────────────────────────────────────────────
// parse_constructors
// ─────────────────────────────────────────────────────────────────────

#[test]
fn constructors_map_to_the_kind_they_build() {
    let ctors = parse_constructors(CTORS);
    assert!(ctors.contains(&("expected_function".into(), "ExpectedFunction".into())));
}

/// The kind can sit lines below its `pub fn` — the scan must carry the
/// pending name across, or every multi-line constructor is lost.
#[test]
fn a_multi_line_constructor_still_resolves() {
    let ctors = parse_constructors(CTORS);
    assert!(
        ctors.contains(&("module_not_found".into(), "ModuleNotFound".into())),
        "{ctors:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// resolve_query
// ─────────────────────────────────────────────────────────────────────

#[test]
fn a_code_resolves_case_insensitively() {
    let pairs = parse_kind_codes(CODES);
    let expected = Some(("ExpectedFunction".to_string(), "E0013".to_string()));
    assert_eq!(resolve_query("E0013", &pairs), expected);
    assert_eq!(resolve_query("e0013", &pairs), expected, "lower case");
    assert_eq!(resolve_query("  E0013 ", &pairs), expected, "padded");
}

/// The kind name works too — the reverse lookup is the whole point when you
/// have a variant from a `match` arm and want its sites.
#[test]
fn a_kind_name_resolves_as_well_as_a_code() {
    let pairs = parse_kind_codes(CODES);
    assert_eq!(
        resolve_query("expectedfunction", &pairs),
        Some(("ExpectedFunction".to_string(), "E0013".to_string()))
    );
}

#[test]
fn an_unknown_query_resolves_to_nothing() {
    let pairs = parse_kind_codes(CODES);
    assert_eq!(resolve_query("E9999", &pairs), None);
    assert_eq!(resolve_query("", &pairs), None);
}
