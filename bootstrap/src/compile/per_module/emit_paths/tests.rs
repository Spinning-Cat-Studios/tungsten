//! Tests for the shared `--emit-llvm` destination rule.
//!
//! These run on plain paths — no elaboration, no codegen — which is the point of
//! taking [`UnitOrigin`] rather than a `ModuleCodegenUnit`.

use super::*;

fn dest(source_file: &str, def_name: &str, source_root: &str, output_dir: &str) -> String {
    emit_llvm_dest(
        &UnitOrigin {
            source_file: Path::new(source_file),
            def_name,
        },
        Path::new(source_root),
        Path::new(output_dir),
    )
    .expect("in-root")
    .display()
    .to_string()
}

#[test]
fn destination_mirrors_the_source_tree_under_the_output_dir() {
    assert_eq!(
        dest(
            "src/compiler/lexer/token.tg",
            "token_new",
            "src/compiler",
            "target/ll"
        ),
        "target/ll/lexer/token/token_new.ll",
        "the source file becomes a directory and the def becomes the file"
    );
    // A definition in the entry file itself still gets a directory.
    assert_eq!(
        dest("src/compiler/main.tg", "run", "src/compiler", "target/ll"),
        "target/ll/main/run.ll"
    );
}

/// `main` is renamed to `tungsten_main` on the way to LLVM, and this rule owns
/// that rename — a caller that applied it too would double-apply or forget.
#[test]
fn the_entry_point_is_renamed_by_this_rule_not_its_caller() {
    assert_eq!(
        dest("src/compiler/main.tg", "main", "src/compiler", "target/ll"),
        "target/ll/main/tungsten_main.ll"
    );
}

#[test]
fn a_source_file_outside_the_root_has_no_mirror_path() {
    let err = emit_llvm_dest(
        &UnitOrigin {
            source_file: Path::new("/elsewhere/stray.tg"),
            def_name: "f",
        },
        Path::new("src/compiler"),
        Path::new("target/ll"),
    )
    .expect_err("outside the root");
    assert_eq!(
        err,
        OutsideSourceRoot {
            source_file: "/elsewhere/stray.tg".into(),
            source_root: "src/compiler".into(),
        }
    );
    // The message names both paths, since either could be the mistake.
    let rendered = err.to_string();
    assert!(rendered.contains("/elsewhere/stray.tg"), "{rendered}");
    assert!(rendered.contains("src/compiler"), "{rendered}");
}

#[test]
fn the_mono_depot_lands_at_the_output_root() {
    assert_eq!(
        mono_depot_dest(Path::new("target/ll"), "__mono")
            .display()
            .to_string(),
        "target/ll/__mono.ll",
        "no source file, so no mirror directory"
    );
}

/// Two definitions whose names differ only in case map to paths that differ
/// only in case — which a case-insensitive filesystem cannot keep apart. The
/// rule itself is correct; `info codegen unit-paths` is what makes the
/// consequence visible (ADR 28.7.26e retrospective).
#[test]
fn names_differing_only_in_case_produce_paths_differing_only_in_case() {
    let upper = dest(
        "src/compiler/lexer/scanner/chars.tg",
        "char_A",
        "src/compiler",
        "target/ll",
    );
    let lower = dest(
        "src/compiler/lexer/scanner/chars.tg",
        "char_a",
        "src/compiler",
        "target/ll",
    );
    assert_ne!(upper, lower, "the rule distinguishes them");
    assert_eq!(
        upper.to_lowercase(),
        lower.to_lowercase(),
        "but only by case, so APFS/NTFS collapses them"
    );
}
