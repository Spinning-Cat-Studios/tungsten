//! Tests for `doctor check link extern-symbols`.
//!
//! Everything decidable is a pure function over injected text, so the
//! recogniser, the three-way classification and the wording are all assertable
//! without a crate to scan or an exit code to spawn. The two effectful cases
//! that matter — a bad `--core-root` and an empty corpus — are covered by
//! asserting the *report* they produce, because "0 examined" and "0 findings"
//! must not render alike.

use super::report::{classify, render, SymbolReport};
use super::scan::{exports_in_source, index_by_symbol, is_conditional, ExportedSymbol};
use super::*;

mod verdict;

pub(super) fn declared(symbol: &str) -> DeclaredExtern {
    DeclaredExtern {
        symbol: symbol.to_string(),
        file: "src/compiler/elab/ffi/positivity.tg".to_string(),
        offset: 42,
    }
}

pub(super) fn exported(symbol: &str, cfg: Option<&str>) -> ExportedSymbol {
    ExportedSymbol {
        symbol: symbol.to_string(),
        file: "tungsten_core/src/ffi/mod.rs".to_string(),
        cfg: cfg.map(str::to_string),
    }
}

// ---------------------------------------------------------------------------
// The recogniser
// ---------------------------------------------------------------------------

#[test]
fn a_no_mangle_extern_fn_is_an_export() {
    let source = "#[no_mangle]\npub extern \"C\" fn tg_positivity_check() -> u64 {\n    0\n}\n";
    let found = exports_in_source(source, "f.rs");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].symbol, "tg_positivity_check");
    assert_eq!(found[0].cfg, None);
}

#[test]
fn unsafe_and_intervening_attributes_do_not_hide_the_export() {
    let source = "#[no_mangle]\n#[allow(clippy::missing_safety_doc)]\n\
                  pub unsafe extern \"C\" fn tg_positivity_def_begin(n: *const c_char) -> bool {\n}\n";
    assert_eq!(
        exports_in_source(source, "f.rs")[0].symbol,
        "tg_positivity_def_begin"
    );
}

/// **The one that matters for soundness.** `extern "C" { … }` blocks *import*
/// symbols; counting one as an export would turn a real link failure into a
/// green run, which is the exact defect this check exists to catch.
#[test]
fn an_extern_block_declares_nothing_and_is_not_an_export() {
    let source = "extern \"C\" {\n    fn _Exit(code: i32) -> !;\n}\n";
    assert!(exports_in_source(source, "f.rs").is_empty());
}

/// A `#[no_mangle]` on something that is not an `extern "C" fn` — a static, or
/// a plain Rust fn — must not contribute a symbol a `.tg` declaration could
/// falsely resolve against.
#[test]
fn no_mangle_on_a_non_extern_item_is_not_an_export() {
    let source = "#[no_mangle]\npub static TG_VERSION: u32 = 1;\n\
                  #[no_mangle]\npub fn not_c_abi() {}\n";
    assert!(exports_in_source(source, "f.rs").is_empty());
}

#[test]
fn a_cfg_attribute_is_carried_onto_the_export_it_guards() {
    let source = "#[cfg(unix)]\n#[no_mangle]\npub extern \"C\" fn tg_exit(c: i32) -> ! {}\n";
    assert_eq!(
        exports_in_source(source, "f.rs")[0].cfg.as_deref(),
        Some("unix")
    );
}

/// A `#[cfg]` that guarded something *else* must not leak onto the next
/// export — otherwise an unconditional symbol reads as target-dependent.
#[test]
fn a_cfg_on_an_unrelated_item_does_not_leak_forward() {
    let source = "#[cfg(test)]\nmod tests {}\n\n\
                  #[no_mangle]\npub extern \"C\" fn tg_init() {}\n";
    let found = exports_in_source(source, "f.rs");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].cfg, None, "the module's gate leaked onto tg_init");
}

// ---------------------------------------------------------------------------
// Conditional exports
// ---------------------------------------------------------------------------

#[test]
fn an_ungated_declaration_makes_the_symbol_unconditional() {
    assert!(!is_conditional(&[exported("tg_init", None)]));
    assert!(!is_conditional(&[
        exported("tg_init", Some("unix")),
        exported("tg_init", None),
    ]));
}

/// The `unix` / `not(unix)` pair covers every target between them, so the one
/// symbol this crate deliberately splits must NOT report as at-risk. Answering
/// per-declaration would get exactly this case backwards.
#[test]
fn complementary_gates_cover_every_target() {
    assert!(!is_conditional(&[
        exported("tg_exit", Some("unix")),
        exported("tg_exit", Some("not(unix)")),
    ]));
}

#[test]
fn a_single_gate_leaves_the_symbol_conditional() {
    assert!(is_conditional(&[exported(
        "tg_wasm_only",
        Some("target_arch = \"wasm32\"")
    ),]));
    // Two gates that are not complements cover nothing between them.
    assert!(is_conditional(&[
        exported("tg_two", Some("unix")),
        exported("tg_two", Some("windows")),
    ]));
}

#[test]
fn an_empty_declaration_set_is_not_conditional() {
    assert!(!is_conditional(&[]));
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

#[test]
fn a_declaration_with_no_export_is_the_finding() {
    let exports = index_by_symbol(vec![exported("tg_init", None)]);
    let report = classify(&[declared("tg_positivity_check")], &exports);

    assert_eq!(report.unresolved.len(), 1);
    assert!(report.resolved.is_empty());
    assert!(report.has_findings());
    assert_eq!(report.examined(), 1);
}

#[test]
fn a_declaration_with_an_ungated_export_resolves() {
    let exports = index_by_symbol(vec![exported("tg_init", None)]);
    let report = classify(&[declared("tg_init")], &exports);

    assert_eq!(report.resolved.len(), 1);
    assert!(!report.has_findings());
}

/// A conditionally-provided symbol is reported but does NOT turn the check
/// red: it links on this target. Folding it into the finding cell would make
/// the check fire on a healthy tree, which trains its reader to ignore it.
#[test]
fn a_conditional_export_is_reported_without_failing_the_check() {
    let exports = index_by_symbol(vec![exported("tg_wasm", Some("target_arch = \"wasm32\""))]);
    let report = classify(&[declared("tg_wasm")], &exports);

    assert_eq!(report.conditional.len(), 1);
    assert_eq!(report.conditional[0].1, "target_arch = \"wasm32\"");
    assert!(!report.has_findings());
    assert_eq!(report.examined(), 1);
}

// ---------------------------------------------------------------------------
// Rendering — where "examined nothing" must not read like "found nothing"
// ---------------------------------------------------------------------------

#[test]
fn an_empty_corpus_says_it_proved_nothing() {
    let rendered = render(&SymbolReport::default(), "missing.tg", 176, false);
    assert!(rendered.contains("Nothing was examined"), "{rendered}");
    assert!(!rendered.starts_with('✓'), "{rendered}");
}

#[test]
fn an_empty_export_scan_says_so_rather_than_blaming_the_file() {
    let report = classify(&[declared("tg_init")], &index_by_symbol(vec![]));
    let rendered = render(&report, "main.tg", 0, false);
    assert!(rendered.contains("--core-root"), "{rendered}");
    assert!(!rendered.contains("undefined reference"), "{rendered}");
}

#[test]
fn a_clean_run_names_both_counts() {
    let exports = index_by_symbol(vec![exported("tg_init", None)]);
    let rendered = render(
        &classify(&[declared("tg_init")], &exports),
        "main.tg",
        176,
        false,
    );
    assert!(rendered.contains("All 1 declared extern(s)"), "{rendered}");
    assert!(rendered.contains("176 scanned"), "{rendered}");
}

#[test]
fn a_finding_names_the_symbol_the_offset_and_the_fix() {
    let report = classify(&[declared("tg_missing")], &index_by_symbol(vec![]));
    let rendered = render(&report, "main.tg", 176, false);
    assert!(rendered.contains("tg_missing"), "{rendered}");
    assert!(rendered.contains("offset 42"), "{rendered}");
    assert!(rendered.contains("undefined reference"), "{rendered}");
    assert!(rendered.contains("devcontainer-build"), "{rendered}");
}

// ---------------------------------------------------------------------------
// The scan, against the real crate
// ---------------------------------------------------------------------------

/// The check is only as good as its recogniser, and the recogniser is only
/// asserted against snippets above. This pins it against the corpus it will
/// actually run on: `tungsten_core` exports the whole `tg_*` surface, so a
/// recogniser that silently stopped matching would show up here as a collapse
/// rather than as a green run over an empty set.
#[test]
fn the_real_core_root_yields_the_whole_tg_surface() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("tungsten_core/src");
    let exports = scan_exports(&root).expect("tungsten_core/src is readable");

    assert!(
        exports.len() > 150,
        "recogniser collapsed: only {} export(s) found",
        exports.len()
    );
    let index = index_by_symbol(exports);
    for symbol in [
        "tg_init",
        "tg_type_nat",
        "tg_positivity_check",
        "tg_positivity_violation_render",
    ] {
        assert!(index.contains_key(symbol), "{symbol} missing from the scan");
    }
}

/// The scan's one soundness caveat, pinned rather than trusted: today exactly
/// one symbol is `#[cfg]`-gated, and its gates are complementary, so nothing in
/// `tungsten_core` is genuinely target-conditional. A new gated export cannot
/// arrive without this failing and forcing someone to decide whether the
/// report's conditional cell is now load-bearing.
#[test]
fn the_only_conditional_export_is_the_unix_split_pair() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("tungsten_core/src");
    let index = index_by_symbol(scan_exports(&root).expect("readable"));

    let conditional: Vec<&String> = index
        .iter()
        .filter(|(_, declarations)| is_conditional(declarations))
        .map(|(symbol, _)| symbol)
        .collect();
    assert!(
        conditional.is_empty(),
        "new target-conditional export(s): {conditional:?}"
    );

    let exit = index.get("tg_exit").expect("tg_exit is exported");
    assert_eq!(exit.len(), 2, "the unix/not(unix) split changed shape");
}
