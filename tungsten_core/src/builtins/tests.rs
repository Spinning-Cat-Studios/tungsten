//! ADR 20.8.26c: the interception tables, their scanners and the asymmetry audit.

use std::collections::BTreeSet;

use super::scan::{
    bootstrap_table, selfhost_builtin_type_names, selfhost_table, tg_definition_names,
};
use super::{
    audit, interception_note, is_bootstrap_intercepted, is_selfhost_intercepted, Reason, Side,
    BOOTSTRAP_INTERCEPTED, DECLARED_ASYMMETRIES, SELFHOST_INTERCEPTED,
};

/// The two sources the constants describe. `include_str!` rather than a runtime
/// read so a move of either file is a compile error here, not a skipped test.
const BOOTSTRAP_SRC: &str = include_str!("../../../bootstrap/src/elaborate/exprs/application.rs");
const SELFHOST_SRC: &str = include_str!("../../../src/compiler/elab/exprs/apply/app/mod.tg");

fn names(entries: &[&str]) -> Vec<String> {
    entries.iter().map(|name| (*name).to_string()).collect()
}

fn defs(entries: &[&str]) -> BTreeSet<String> {
    entries.iter().map(|name| (*name).to_string()).collect()
}

// ---------------------------------------------------------------------------
// The constants are pinned to the sources they describe
// ---------------------------------------------------------------------------

#[test]
fn bootstrap_constant_matches_its_source() {
    assert_eq!(bootstrap_table(BOOTSTRAP_SRC), names(BOOTSTRAP_INTERCEPTED));
}

#[test]
fn selfhost_constant_matches_its_source() {
    assert_eq!(selfhost_table(SELFHOST_SRC), names(SELFHOST_INTERCEPTED));
}

// 18.9.26f AC3: the self-host's builtin-type switch names exactly the shared
// primitive table's types, whatever order it lists them in.
#[test]
fn selfhost_builtin_type_switch_matches_the_primitive_table() {
    let switch_src = include_str!("../../../src/compiler/elab/types/mod.tg");
    let table: BTreeSet<String> = crate::types::PRIMITIVE_TYPES
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();
    assert_eq!(selfhost_builtin_type_names(switch_src), table);
}

#[test]
fn builtin_type_scanner_reads_only_the_switch_body() {
    let src = "fn other(name: String) -> Bool { name == \"Before\" }\n\
               fn elab_builtin_type(name: String) -> T {\n    \
                   if name == \"Nat\" { a } else if name == \"Int\" { b } else { c }\n\
               }\n\
               fn after(name: String) -> Bool { name == \"After\" }\n";
    assert_eq!(selfhost_builtin_type_names(src), defs(&["Int", "Nat"]));
}

#[test]
fn builtin_type_scanner_yields_nothing_without_the_head() {
    assert!(selfhost_builtin_type_names("if name == \"Nat\" { a }\n").is_empty());
}

/// The self-host file's own doc comment names every bootstrap-side entry. If the
/// scanner read bare identifiers instead of the call, the two tables would read
/// as identical — the exact false green this gate exists to prevent.
#[test]
fn selfhost_scanner_ignores_names_that_only_appear_in_prose() {
    assert!(SELFHOST_SRC.contains("`char_at`"));
    assert!(!selfhost_table(SELFHOST_SRC).contains(&"char_at".to_string()));
}

/// Likewise on the bootstrap side: `application.rs` discusses `substring` at
/// length in a doc comment above the match.
#[test]
fn bootstrap_scanner_reads_only_the_match_arms() {
    let table = bootstrap_table(BOOTSTRAP_SRC);
    // 10 → 12 when ADR 14.9.26c added the two `Int` bridges to BOTH tables.
    assert_eq!(table.len(), 12, "table was {table:?}");
    assert_eq!(table.first().map(String::as_str), Some("ref"));
    assert_eq!(table.last().map(String::as_str), Some("from_int"));
}

// ---------------------------------------------------------------------------
// Scanners, over injected sources
// ---------------------------------------------------------------------------

#[test]
fn bootstrap_scanner_stops_at_the_wildcard_arm() {
    let src = r#"
        match path.item_name().name.as_str() {
            "alpha" => return self.elab_alpha(args, span).map(Some),
            "beta" => return self.elab_beta(args, span).map(Some),
            _ => {}
        }
        let unrelated = "gamma";
    "#;
    assert_eq!(bootstrap_table(src), names(&["alpha", "beta"]));
}

#[test]
fn bootstrap_scanner_yields_nothing_when_the_match_head_is_absent() {
    assert!(bootstrap_table("\"alpha\" => return self.x(),\n").is_empty());
}

#[test]
fn bootstrap_scanner_skips_a_literal_that_does_not_open_an_arm() {
    let src = "match path.item_name().name.as_str() {\n\
               \"alpha\" => return self.a(),\n\
               \"not an arm\".to_string();\n\
               _ => {}\n";
    assert_eq!(bootstrap_table(src), names(&["alpha"]));
}

#[test]
fn selfhost_scanner_reads_every_call_on_a_line() {
    let src = "if is_simple_path_named(func, \"a\") { x } \
               else if is_simple_path_named(func, \"b\") { y }";
    assert_eq!(selfhost_table(src), names(&["a", "b"]));
}

#[test]
fn tg_definition_scanner_reads_plain_pub_and_extern_forms() {
    let src = "fn plain(s: String) -> Nat { 0 }\n\
               pub fn exported(s: String) -> Nat { 0 }\n\
               extern \"C\" fn tg_raw(s: String) -> Nat\n\
               pub extern \"C\" fn tg_pub_raw(s: String) -> Nat\n\
               // fn commented_out() -> Nat\n";
    assert_eq!(
        tg_definition_names(src),
        defs(&["plain", "exported", "tg_raw", "tg_pub_raw"])
    );
}

/// The live claim the `no-tg-fallback` declarations rest on.
#[test]
fn only_string_len_among_the_declared_names_has_a_tg_definition() {
    let types_tg = include_str!("../../../src/compiler/driver/ffi/types/mod.tg");
    let found = tg_definition_names(types_tg);
    assert!(found.contains("string_len"));
    for absent in ["ref", "get", "set", "char_at"] {
        assert!(!found.contains(absent), "{absent} unexpectedly defined");
    }
}

// ---------------------------------------------------------------------------
// The audit
// ---------------------------------------------------------------------------

#[test]
fn identical_tables_have_no_asymmetries() {
    let table = names(&["a", "b"]);
    assert!(audit(&table, &table, &defs(&[])).is_empty());
}

#[test]
fn an_undeclared_bootstrap_only_name_fails() {
    let found = audit(&names(&["a", "newcomer"]), &names(&["a"]), &defs(&[]));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "newcomer");
    assert_eq!(found[0].side, Side::BootstrapOnly);
    assert!(found[0].is_failure());
    assert!(found[0].failure.as_deref().unwrap().contains("undeclared"));
}

/// AC4's second direction: removing an entry from the SELF-HOST's table is an
/// asymmetry too, and reports as such rather than as a bootstrap gap.
#[test]
fn an_undeclared_selfhost_only_name_fails() {
    let found = audit(&names(&["a"]), &names(&["a", "newcomer"]), &defs(&[]));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].side, Side::SelfHostOnly);
    assert!(found[0].is_failure());
}

#[test]
fn a_no_tg_fallback_declaration_passes_while_no_definition_exists() {
    let found = audit(
        &names(&["char_at"]),
        &names(&[]),
        &defs(&["something_else"]),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].declared, Some(Reason::NoTgFallback));
    assert!(!found[0].is_failure());
}

/// The rule that would have caught this ADR's defect on the day
/// `build/mod.tg` first defined `substring`.
#[test]
fn a_no_tg_fallback_declaration_fails_once_a_tg_definition_appears() {
    let found = audit(&names(&["char_at"]), &names(&[]), &defs(&["char_at"]));
    assert_eq!(found.len(), 1);
    assert!(found[0].is_failure());
    assert!(found[0]
        .failure
        .as_deref()
        .unwrap()
        .contains("now defines `char_at`"));
}

#[test]
fn a_matching_semantics_declaration_fails_once_its_definition_disappears() {
    let found = audit(&names(&["string_len"]), &names(&[]), &defs(&[]));
    assert_eq!(found.len(), 1);
    assert!(found[0].is_failure());
    assert!(found[0]
        .failure
        .as_deref()
        .unwrap()
        .contains("no longer defines"));
}

#[test]
fn a_matching_semantics_declaration_passes_while_its_definition_stands() {
    let found = audit(&names(&["string_len"]), &names(&[]), &defs(&["string_len"]));
    assert_eq!(found.len(), 1);
    assert!(!found[0].is_failure());
}

#[test]
fn a_spelling_alias_holds_only_while_its_twin_is_in_both_tables() {
    let held = audit(
        &names(&["__compare", "compare"]),
        &names(&["compare"]),
        &defs(&[]),
    );
    assert_eq!(held.len(), 1);
    assert!(!held[0].is_failure());

    let broken = audit(&names(&["__compare", "compare"]), &names(&[]), &defs(&[]));
    let alias = broken.iter().find(|a| a.name == "__compare").unwrap();
    assert!(alias.is_failure());
    assert!(alias
        .failure
        .as_deref()
        .unwrap()
        .contains("no longer in both"));
}

/// The reconciled pair: the tables this repo actually ships, audited against the
/// definitions it actually has, must be green.
#[test]
fn the_shipped_tables_are_reconciled() {
    let tg_defs = defs(&["string_len"]);
    let failures: Vec<_> = audit(
        &names(BOOTSTRAP_INTERCEPTED),
        &names(SELFHOST_INTERCEPTED),
        &tg_defs,
    )
    .into_iter()
    .filter(super::Asymmetry::is_failure)
    .collect();
    assert!(failures.is_empty(), "unreconciled: {failures:?}");
}

/// AC4, both directions, against the real tables rather than a fixture.
#[test]
fn removing_substring_from_either_shipped_table_goes_red() {
    let tg_defs = defs(&["string_len"]);
    let without = |table: &[&str]| -> Vec<String> {
        table
            .iter()
            .filter(|n| **n != "substring")
            .map(|n| (*n).to_string())
            .collect()
    };

    let boot_gap = audit(
        &without(BOOTSTRAP_INTERCEPTED),
        &names(SELFHOST_INTERCEPTED),
        &tg_defs,
    );
    assert!(boot_gap.iter().any(super::Asymmetry::is_failure));

    let sh_gap = audit(
        &names(BOOTSTRAP_INTERCEPTED),
        &without(SELFHOST_INTERCEPTED),
        &tg_defs,
    );
    assert!(sh_gap.iter().any(super::Asymmetry::is_failure));
}

#[test]
fn every_declared_name_is_in_exactly_one_shipped_table() {
    for (name, _) in DECLARED_ASYMMETRIES {
        let in_boot = BOOTSTRAP_INTERCEPTED.contains(name);
        let in_sh = SELFHOST_INTERCEPTED.contains(name);
        assert!(
            in_boot != in_sh,
            "`{name}` is declared asymmetric but is in {}",
            if in_boot {
                "both tables"
            } else {
                "neither table"
            }
        );
    }
}

// ---------------------------------------------------------------------------
// The predicate and the note
// ---------------------------------------------------------------------------

#[test]
fn the_union_is_sorted_deduped_and_holds_both_tables() {
    let all = super::all_intercepted_names();
    assert_eq!(all.len(), 12, "{all:?}");

    let mut sorted = all.clone();
    sorted.sort_unstable();
    assert_eq!(all, sorted, "the listing depends on a stable order");

    let mut deduped = all.clone();
    deduped.dedup();
    assert_eq!(
        all, deduped,
        "`compare` is in both tables and must appear once"
    );

    for name in BOOTSTRAP_INTERCEPTED.iter().chain(SELFHOST_INTERCEPTED) {
        assert!(all.contains(name), "{name} missing from the union");
    }
}

#[test]
fn interception_predicates_answer_for_each_table() {
    assert!(is_bootstrap_intercepted("substring"));
    assert!(is_selfhost_intercepted("substring"));
    assert!(is_bootstrap_intercepted("char_at"));
    assert!(!is_selfhost_intercepted("char_at"));
    assert!(!is_bootstrap_intercepted("lex_slice"));
}

#[test]
fn an_ordinary_name_gets_no_note() {
    assert_eq!(interception_note("lex_slice"), None);
}

#[test]
fn a_one_sided_name_says_the_self_host_still_resolves_here() {
    let note = interception_note("char_at").expect("char_at is intercepted");
    assert!(note.contains("does NOT"), "{note}");
    assert!(note.contains("bare name"), "{note}");
}

#[test]
fn a_two_sided_name_says_neither_compiler_resolves_here() {
    let note = interception_note("substring").expect("substring is intercepted");
    assert!(note.contains("BOTH compilers"), "{note}");
    assert!(note.contains("neither ever resolves"), "{note}");
}

#[test]
fn slugs_are_distinct_per_reason() {
    assert_eq!(Reason::NoTgFallback.slug(), "no-tg-fallback");
    assert_eq!(Reason::MatchingTgSemantics.slug(), "matching-tg-semantics");
    assert_eq!(Reason::SpellingAlias("compare").slug(), "spelling-alias");
}

#[test]
fn side_labels_are_distinct() {
    assert_ne!(Side::BootstrapOnly.label(), Side::SelfHostOnly.label());
}
