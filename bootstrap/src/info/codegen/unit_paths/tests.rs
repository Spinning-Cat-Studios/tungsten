//! Tests for `info codegen unit-paths`: the collision planner, its tally, and
//! the exit contract. All run on plain paths — no elaboration, no codegen.

use super::*;

/// Build origins from `(source_file, def_name)` pairs.
fn origins<'a>(pairs: &'a [(&'a str, &'a str)]) -> Vec<UnitOrigin<'a>> {
    pairs
        .iter()
        .map(|(source_file, def_name)| UnitOrigin {
            source_file: Path::new(source_file),
            def_name,
        })
        .collect()
}

fn plan(pairs: &[(&str, &str)]) -> UnitPathPlan {
    plan_unit_paths(&origins(pairs), Path::new("src/compiler"), Path::new("out"))
}

#[test]
fn distinct_units_get_distinct_paths_and_no_collisions() {
    let plan = plan(&[
        ("src/compiler/lexer/token.tg", "token_new"),
        ("src/compiler/lexer/token.tg", "token_kind"),
        ("src/compiler/parser/lib.tg", "parse"),
    ]);
    assert!(plan.collisions.is_empty(), "{:?}", plan.collisions);
    assert_eq!(
        plan.tally(),
        UnitPathTally {
            units: 3,
            distinct_paths: 3,
            exact_collisions: 0,
            case_collisions: 0,
            unplaceable: 0,
        }
    );
    assert!(plan.tally().is_clean());
}

/// The defect that motivated the tool: two defs differing only in case, which
/// APFS cannot keep apart. Six such pairs silently cost the self-hosted
/// compiler's corpus six `.ll` files (ADR 28.7.26e §1.2).
#[test]
fn names_differing_only_in_case_are_a_case_only_collision() {
    let plan = plan(&[
        ("src/compiler/lexer/scanner/chars.tg", "char_A"),
        ("src/compiler/lexer/scanner/chars.tg", "char_a"),
    ]);
    assert_eq!(plan.collisions.len(), 1);
    let collision = &plan.collisions[0];
    assert_eq!(collision.kind, CollisionKind::CaseOnly);
    assert_eq!(collision.units.len(), 2);
    let defs: Vec<&str> = collision
        .units
        .iter()
        .map(|u| u.def_name.as_str())
        .collect();
    assert!(
        defs.contains(&"char_A") && defs.contains(&"char_a"),
        "{defs:?}"
    );

    assert_eq!(
        plan.tally(),
        UnitPathTally {
            units: 2,
            // Two units, ONE path a case-insensitive filesystem will hold.
            distinct_paths: 1,
            exact_collisions: 0,
            case_collisions: 1,
            unplaceable: 0,
        }
    );
    assert!(!plan.tally().is_clean());
}

/// Two units whose destinations match byte-for-byte are broken on every
/// filesystem, so they outrank a case-only finding.
#[test]
fn identical_paths_are_an_exact_collision() {
    // Same source file, same def name — the emitter would write one file twice.
    let plan = plan(&[
        ("src/compiler/lexer/token.tg", "token_new"),
        ("src/compiler/lexer/token.tg", "token_new"),
    ]);
    assert_eq!(plan.collisions.len(), 1);
    assert_eq!(plan.collisions[0].kind, CollisionKind::Exact);
    assert_eq!(
        plan.tally(),
        UnitPathTally {
            units: 2,
            distinct_paths: 1,
            exact_collisions: 1,
            case_collisions: 0,
            unplaceable: 0,
        }
    );
}

/// A mixed group reports as exact: it is broken everywhere, not only on APFS.
#[test]
fn a_group_holding_both_kinds_reports_the_stronger_one() {
    let plan = plan(&[
        ("src/compiler/x.tg", "f"),
        ("src/compiler/x.tg", "f"),
        ("src/compiler/x.tg", "F"),
    ]);
    assert_eq!(plan.collisions.len(), 1);
    assert_eq!(
        plan.collisions[0].kind,
        CollisionKind::Exact,
        "an exact duplicate inside the group wins"
    );
    assert_eq!(plan.collisions[0].units.len(), 3);
}

/// `main` is renamed on the way to LLVM, so `main` and a hand-written
/// `tungsten_main` in the same file DO collide — a case the raw def names hide.
#[test]
fn the_entry_point_rename_can_itself_create_a_collision() {
    let plan = plan(&[
        ("src/compiler/main.tg", "main"),
        ("src/compiler/main.tg", "tungsten_main"),
    ]);
    assert_eq!(
        plan.collisions.len(),
        1,
        "both land on main/tungsten_main.ll: {:?}",
        plan.placed
    );
    assert_eq!(plan.collisions[0].kind, CollisionKind::Exact);
}

#[test]
fn a_unit_outside_the_source_root_is_unplaceable_not_colliding() {
    let plan = plan(&[
        ("src/compiler/lexer/token.tg", "token_new"),
        ("/elsewhere/stray.tg", "stray"),
    ]);
    assert!(plan.collisions.is_empty());
    assert_eq!(plan.unplaceable.len(), 1);
    assert_eq!(
        plan.tally(),
        UnitPathTally {
            units: 2,
            distinct_paths: 1,
            exact_collisions: 0,
            case_collisions: 0,
            unplaceable: 1,
        },
        "the unplaceable unit counts toward `units` but has no path"
    );
    assert!(
        !plan.tally().is_clean(),
        "the emitter hard-errors on this, so the diagnostic must not call it clean"
    );
}

#[test]
fn no_units_is_clean_and_empty() {
    let plan = plan(&[]);
    assert_eq!(plan, UnitPathPlan::default());
    assert_eq!(plan.tally(), UnitPathTally::default());
    assert!(plan.tally().is_clean());
}

/// Collision groups come back in a deterministic order — this is diagnostic
/// output that gets diffed between runs.
#[test]
fn collision_order_is_deterministic() {
    let pairs = [
        ("src/compiler/z.tg", "Q"),
        ("src/compiler/z.tg", "q"),
        ("src/compiler/a.tg", "P"),
        ("src/compiler/a.tg", "p"),
    ];
    let first: Vec<String> = plan(&pairs)
        .collisions
        .iter()
        .map(|c| c.dest.display().to_string())
        .collect();
    assert_eq!(first.len(), 2);
    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(first, sorted, "grouped by a sorted key");
    // And stable across repeated planning of the same input.
    let second: Vec<String> = plan(&pairs)
        .collisions
        .iter()
        .map(|c| c.dest.display().to_string())
        .collect();
    assert_eq!(first, second);
}

/// Only a clean tally exits successfully — the whole-body `-> Default::default()`
/// mutant would otherwise pass, and `ExitCode::default()` is SUCCESS.
#[test]
fn only_a_clean_tally_exits_successfully() {
    let clean = format!("{:?}", UnitPathTally::default().exit());
    assert_eq!(clean, format!("{:?}", ExitCode::SUCCESS));
    for dirty in [
        UnitPathTally {
            exact_collisions: 1,
            ..UnitPathTally::default()
        },
        UnitPathTally {
            case_collisions: 1,
            ..UnitPathTally::default()
        },
        UnitPathTally {
            unplaceable: 1,
            ..UnitPathTally::default()
        },
    ] {
        assert!(!dirty.is_clean(), "{dirty:?}");
        assert_ne!(format!("{:?}", dirty.exit()), clean, "{dirty:?}");
    }
    // Units and distinct-path counts alone never make a tally dirty.
    assert!(UnitPathTally {
        units: 9,
        distinct_paths: 9,
        ..UnitPathTally::default()
    }
    .is_clean());
}

#[test]
fn each_collision_kind_explains_itself_distinctly() {
    let exact = CollisionKind::Exact.human();
    let case_only = CollisionKind::CaseOnly.human();
    assert!(exact.contains("every filesystem"), "{exact}");
    assert!(case_only.contains("case"), "{case_only}");
    assert_ne!(exact, case_only);
}

// ── The rendered surfaces ───────────────────────────────────────────────────

/// Both renderers run over every shape without panicking, and the JSON one
/// produces parseable output whose counts match the tally.
#[test]
fn json_output_is_parseable_and_agrees_with_the_tally() {
    let plan = plan(&[
        ("src/compiler/lexer/scanner/chars.tg", "char_A"),
        ("src/compiler/lexer/scanner/chars.tg", "char_a"),
        ("/elsewhere/stray.tg", "stray"),
    ]);
    let out = Path::new("out");
    print_human(&plan, out);

    let rendered = json_value(&plan, out);
    assert_eq!(rendered["units"], 3);
    assert_eq!(rendered["case_collisions"], 1);
    assert_eq!(rendered["exact_collisions"], 0);
    assert_eq!(rendered["unplaceable"].as_array().unwrap().len(), 1);
    assert_eq!(rendered["collisions"][0]["kind"], "case-only");
    assert_eq!(
        rendered["collisions"][0]["units"].as_array().unwrap().len(),
        2
    );
    assert_eq!(rendered["output_dir"], "out");
}

#[test]
fn json_names_the_exact_kind_too() {
    let plan = plan(&[("src/compiler/x.tg", "f"), ("src/compiler/x.tg", "f")]);
    assert_eq!(
        json_value(&plan, Path::new("out"))["collisions"][0]["kind"],
        "exact"
    );
}

#[test]
fn a_nonexistent_entry_file_is_bad_input() {
    let path = std::path::PathBuf::from("/nonexistent/28726f.tg");
    let clean = format!("{:?}", ExitCode::SUCCESS);
    assert_ne!(
        format!(
            "{:?}",
            cmd_info_codegen_unit_paths(&path, None, false, false, 20)
        ),
        clean
    );
}
