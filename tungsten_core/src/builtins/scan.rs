//! Reading the two interception tables out of the sources that hold them.
//!
//! Neither table is data in its own compiler — the bootstrap's is a `match` on
//! string literals that dispatches to ten different methods, the self-host's a
//! chain of `is_simple_path_named` calls — and turning either into data would
//! mean a dispatch rewrite for no benefit the gate could not get more cheaply.
//! So the gate reads the source, and the constants in the parent module are
//! pinned to what these scanners find.
//!
//! All three functions are pure over an injected `&str`, which is what makes the
//! gate's own tests able to assert on tables no file contains.

use std::collections::BTreeSet;

/// The line that opens the bootstrap's interception `match`.
const BOOTSTRAP_MATCH_HEAD: &str = "match path.item_name().name.as_str() {";

/// The arm that closes it.
const BOOTSTRAP_MATCH_TAIL: &str = "_ => {}";

/// Names intercepted by `try_elab_special_application`, in source order.
///
/// Reads only the arms of the one `match` that shadows the value environment;
/// a string literal elsewhere in the file — a doc comment naming `substring`,
/// say, of which that file has several — is deliberately not a table entry.
#[must_use]
pub fn bootstrap_table(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in src.lines() {
        let trimmed = line.trim();
        if !inside {
            inside = trimmed.ends_with(BOOTSTRAP_MATCH_HEAD);
            continue;
        }
        if trimmed.starts_with(BOOTSTRAP_MATCH_TAIL) {
            break;
        }
        if let Some(name) = leading_string_literal(trimmed) {
            if trimmed[name.len() + 2..].trim_start().starts_with("=>") {
                out.push(name);
            }
        }
    }
    out
}

/// Names intercepted by `synth_app`, in source order.
///
/// Matches the call, not the identifier: the file's own doc comment names every
/// bootstrap-side entry, and reading those as table entries would make the gate
/// report the tables as identical for exactly the reason they are not.
#[must_use]
pub fn selfhost_table(src: &str) -> Vec<String> {
    const CALL: &str = "is_simple_path_named(func, \"";
    let mut out = Vec::new();
    for line in src.lines() {
        let mut rest = line;
        while let Some(at) = rest.find(CALL) {
            rest = &rest[at + CALL.len()..];
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
            }
        }
    }
    out
}

/// Top-level `.tg` function names declared in one file.
///
/// The gate unions this over `src/compiler` to answer the question a table
/// comparison cannot: does the name the bootstrap intercepts *resolve* to
/// something on the self-host side? Extern declarations count — they are
/// resolvable names too.
#[must_use]
pub fn tg_definition_names(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in src.lines() {
        let mut rest = line.trim();
        // Applied in declaration order, so `pub extern "C" fn` is peeled by two
        // passes rather than needing a third, combined prefix.
        for prefix in ["pub ", "extern \"C\" "] {
            if let Some(stripped) = rest.strip_prefix(prefix) {
                rest = stripped.trim_start();
            }
        }
        let Some(after_fn) = rest.strip_prefix("fn ") else {
            continue;
        };
        let name: String = after_fn
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            out.insert(name);
        }
    }
    out
}

/// Names the self-host's `elab_builtin_type` switch resolves to a builtin type.
///
/// Reads every `name == "…"` comparison between the function's head and the
/// first line that closes it at column zero, so a literal elsewhere in the file
/// is not a table entry. Order-free: the self-host keeps its own ordering for
/// its comments' sake, and the reconciliation is a set comparison (ADR 18.9.26f).
#[must_use]
pub fn selfhost_builtin_type_names(src: &str) -> BTreeSet<String> {
    const HEAD: &str = "fn elab_builtin_type(";
    const COMPARISON: &str = "name == \"";
    let mut builtin_names = BTreeSet::new();
    let mut inside = false;
    for line in src.lines() {
        if !inside {
            inside = line.trim_start().starts_with(HEAD);
            continue;
        }
        if line.starts_with('}') {
            break;
        }
        let mut rest = line;
        while let Some(at) = rest.find(COMPARISON) {
            rest = &rest[at + COMPARISON.len()..];
            if let Some(end) = rest.find('"') {
                builtin_names.insert(rest[..end].to_string());
            }
        }
    }
    builtin_names
}

/// The `"name"` at the start of `trimmed`, if it opens with a string literal.
fn leading_string_literal(trimmed: &str) -> Option<String> {
    let body = trimmed.strip_prefix('"')?;
    let end = body.find('"')?;
    Some(body[..end].to_string())
}
