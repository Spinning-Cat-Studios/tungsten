//! The pure half: which C symbols `tungsten_core` actually exports.
//!
//! A **source** scan rather than a symbol-table read, deliberately. Reading
//! `libtungsten_core.a` would mean either shelling out to `nm` or carrying an
//! archive parser, and both make the check depend on a build having happened —
//! which is precisely the thing it exists to run *before*. Scanning source
//! costs a directory walk and answers at cost 2, on any machine, with no
//! container and no LLVM.
//!
//! ## What that costs, stated rather than hidden
//!
//! A source scan sees a `#[cfg]`-gated export as present. So an export gated to
//! one target is reported as *conditionally* provided rather than provided —
//! [`ExportedSymbol::cfg`] carries the gate, and the report separates the two
//! cells. Today `tungsten_core` has exactly one such symbol, `tg_exit`, split
//! across a `unix` / `not(unix)` pair so that it is present on every target;
//! `tests::the_only_conditional_export_is_the_unix_split_pair` pins that, so a
//! new gated export cannot arrive unnoticed.

use std::collections::BTreeMap;

/// One `#[no_mangle] extern "C"` symbol found in `tungsten_core`'s sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportedSymbol {
    /// The C symbol name — what a `.tg` `extern "C"` declaration must match.
    pub symbol: String,
    /// Repo-relative file the export was found in.
    pub file: String,
    /// The `#[cfg(…)]` guarding it, if any. `Some` means "provided on some
    /// targets", which is a different answer from "provided".
    pub cfg: Option<String>,
}

/// Every export declared in one Rust source file.
///
/// A pure function over the file's text, so the recogniser is assertable
/// against hand-written snippets rather than against whatever the crate
/// happens to contain today.
///
/// The recogniser is deliberately narrow: `#[no_mangle]` followed, after any
/// number of further attributes, by a line declaring `extern "C" fn <name>`.
/// A wider one would start matching the `extern "C" { … }` *declaration* blocks
/// this crate also contains — those import symbols rather than export them, and
/// counting one as an export would turn a real link failure into a green run.
pub(crate) fn exports_in_source(text: &str, file: &str) -> Vec<ExportedSymbol> {
    let mut found = Vec::new();
    let mut pending_cfg: Option<String> = None;
    let mut saw_no_mangle = false;

    for line in text.lines() {
        let trimmed = line.trim();

        // Blank lines and comments are FORMATTING inside an attribute run, not
        // the item it guards. Consuming one as the item is how a `#[no_mangle]`
        // separated from its `fn` by a blank line stops being recognised — and
        // the export then reads as absent, which is a false *finding* against
        // whichever `.tg` file declares it.
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if let Some(cfg) = trimmed
            .strip_prefix("#[cfg(")
            .and_then(|r| r.strip_suffix(")]"))
        {
            pending_cfg = Some(cfg.to_string());
            continue;
        }
        if trimmed == "#[no_mangle]" {
            saw_no_mangle = true;
            continue;
        }
        if trimmed.starts_with("#[") {
            // Another attribute between the two: keep looking.
            continue;
        }

        if saw_no_mangle {
            if let Some(symbol) = extern_fn_name(trimmed) {
                found.push(ExportedSymbol {
                    symbol,
                    file: file.to_string(),
                    cfg: pending_cfg.clone(),
                });
            }
            saw_no_mangle = false;
        }
        // Any other code ends the attribute run: a `#[cfg]` that guarded
        // something else must not leak onto the next export.
        pending_cfg = None;
    }
    found
}

/// The symbol a `pub [unsafe] extern "C" fn <name>(…)` line declares.
///
/// `None` for anything else — including `extern "C" {` blocks, which import.
fn extern_fn_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("pub ")?;
    let rest = rest.strip_prefix("unsafe ").unwrap_or(rest);
    let rest = rest.strip_prefix("extern \"C\" fn ")?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Index exports by symbol. A symbol declared more than once (the `unix` /
/// `not(unix)` split) keeps every declaration, because "is it gated?" can only
/// be answered by looking at all of them.
pub(crate) fn index_by_symbol(
    exports: Vec<ExportedSymbol>,
) -> BTreeMap<String, Vec<ExportedSymbol>> {
    let mut index: BTreeMap<String, Vec<ExportedSymbol>> = BTreeMap::new();
    for export in exports {
        index.entry(export.symbol.clone()).or_default().push(export);
    }
    index
}

/// Whether a symbol might be **absent** on some target.
///
/// One ungated declaration means present everywhere. All-gated means
/// conditional — *unless* two of the gates are complementary (`X` and
/// `not(X)`), which is how `tg_exit` provides one symbol through two
/// target-specific bodies. Answering per-declaration instead would report the
/// one symbol this crate deliberately splits as the one at risk, which is the
/// wrong way round.
pub(crate) fn is_conditional(declarations: &[ExportedSymbol]) -> bool {
    if declarations.is_empty() || declarations.iter().any(|export| export.cfg.is_none()) {
        return false;
    }
    !has_complementary_gates(declarations)
}

/// Whether two of these gates are `X` and `not(X)`, and so cover every target
/// between them.
fn has_complementary_gates(declarations: &[ExportedSymbol]) -> bool {
    let gates: Vec<&str> = declarations
        .iter()
        .filter_map(|export| export.cfg.as_deref())
        .collect();
    gates
        .iter()
        .any(|gate| gates.iter().any(|other| negates(other, gate)))
}

/// Whether `candidate` is exactly `not(<gate>)`.
fn negates(candidate: &str, gate: &str) -> bool {
    candidate
        .strip_prefix("not(")
        .and_then(|inner| inner.strip_suffix(')'))
        .is_some_and(|inner| inner == gate)
}
