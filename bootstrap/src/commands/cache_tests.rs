//! Tests for `cache clean-project` and `cache prune` (ADR 5.8.26d follow-up).
//!
//! Both commands resolved their cache root from the CWD while the writer
//! resolves it from the entry file's parent — the same D5 defect fixed for
//! `cache status`, left behind in two siblings. The tests that matter here are
//! the targeting ones: that an operand reaches the operand's project and NOT
//! the cwd, which is what the bug got wrong.

use super::*;
use std::fs;

/// A project directory with a populated `.tungsten/` cache, plus an entry file.
fn project_with_cache(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let proj = dir.path().join(name);
    fs::create_dir_all(&proj).unwrap();
    let entry = proj.join("main.tg");
    fs::write(&entry, "fn main() -> Nat { 0 }").unwrap();
    // `BuildCache::new` creates the tree; that is enough for clear/prune to act on.
    BuildCache::new(&proj, false).unwrap();
    (dir, entry)
}

#[test]
fn clean_targets_the_operands_project_not_the_cwd() {
    // The D5 defect: with a cwd elsewhere, the operand must still decide.
    let (dir, entry) = project_with_cache("compiler");
    let elsewhere = tempfile::tempdir().unwrap();

    let outcome = run_clean(false, Some(&entry), elsewhere.path());

    match outcome {
        CacheOutcome::Reported(lines) => {
            let joined = lines.join("\n");
            assert!(joined.contains("Cache cleared"), "{joined}");
            assert!(
                joined.contains(
                    &dir.path()
                        .join("compiler")
                        .canonicalize()
                        .unwrap()
                        .display()
                        .to_string()
                ),
                "must name the operand's project: {joined}"
            );
            assert!(
                !joined.contains(&elsewhere.path().display().to_string()),
                "must NOT name the cwd: {joined}"
            );
        }
        CacheOutcome::Failed(m) => panic!("unexpected failure: {m}"),
    }
}

#[test]
fn clean_falls_back_to_the_cwd_when_given_no_operand() {
    let dir = tempfile::tempdir().unwrap();
    let outcome = run_clean(false, None, dir.path());
    match outcome {
        CacheOutcome::Reported(lines) => assert!(
            lines.join("\n").contains(&dir.path().display().to_string()),
            "{lines:?}"
        ),
        CacheOutcome::Failed(m) => panic!("unexpected failure: {m}"),
    }
}

#[test]
fn clean_refuses_an_operand_that_names_nothing() {
    // Refusing beats resolving: `BuildCache::new` would CREATE the directory,
    // so a typo'd path would silently "clear" a cache it had just made.
    let outcome = run_clean(
        false,
        Some(Path::new("/definitely/not/here/main.tg")),
        Path::new("/work/repo"),
    );
    assert_eq!(
        outcome,
        CacheOutcome::Failed("no such file: /definitely/not/here/main.tg".to_string())
    );
}

#[test]
fn the_clean_command_itself_exits_nonzero_on_a_missing_operand() {
    // Covers the CLI shell, not just `run_clean`: without this, a mutant
    // replacing the return with `Default::default()` (== SUCCESS) survives and
    // silently turns a refusal into a success. Depends on no checkout state.
    let code = cmd_clean(false, Some(Path::new("/definitely/not/here/main.tg")));
    assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(3)));
}

#[test]
fn the_prune_command_itself_exits_nonzero_on_a_missing_operand() {
    let code = cmd_cache_prune(false, None, Some(Path::new("/definitely/not/here/main.tg")));
    assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(3)));
}

#[test]
fn prune_targets_the_operands_project_not_the_cwd() {
    let (dir, entry) = project_with_cache("compiler");
    let elsewhere = tempfile::tempdir().unwrap();

    let outcome = run_cache_prune(false, None, Some(&entry), elsewhere.path());

    match outcome {
        CacheOutcome::Reported(lines) => {
            let joined = lines.join("\n");
            assert!(
                joined.contains(
                    &dir.path()
                        .join("compiler")
                        .canonicalize()
                        .unwrap()
                        .display()
                        .to_string()
                ),
                "must name the operand's project: {joined}"
            );
            assert!(
                !joined.contains(&elsewhere.path().display().to_string()),
                "must NOT name the cwd: {joined}"
            );
        }
        CacheOutcome::Failed(m) => panic!("unexpected failure: {m}"),
    }
}

#[test]
fn prune_refuses_an_operand_that_names_nothing() {
    let outcome = run_cache_prune(
        false,
        None,
        Some(Path::new("/definitely/not/here/main.tg")),
        Path::new("/work/repo"),
    );
    assert_eq!(
        outcome,
        CacheOutcome::Failed("no such file: /definitely/not/here/main.tg".to_string())
    );
}

// --- prune rendering -------------------------------------------------------

fn prune_stats(removed: usize, freed: u64, new_size: u64) -> PruneStats {
    PruneStats {
        removed_count: removed,
        freed_bytes: freed,
        new_size_bytes: new_size,
    }
}

#[test]
fn an_empty_prune_reports_already_within_limits_in_kb() {
    // Pins the removed_count == 0 branch and the /1024 conversion.
    let lines = format_prune(&prune_stats(0, 0, 4096), Path::new("/w/proj"));
    assert_eq!(lines[0], "✓ Cache already within limits (4 KB)");
    assert_eq!(lines[1], "  Root:         /w/proj");
}

#[test]
fn a_nonempty_prune_reports_counts_and_both_sizes_in_kb() {
    // 8192 freed = 8 KB, 2048 remaining = 2 KB: pins both divisions and the
    // removed_count != 0 branch.
    let lines = format_prune(&prune_stats(3, 8192, 2048), Path::new("/w/proj"));
    assert_eq!(lines[0], "✓ Pruned 3 entries, freed 8 KB (new size: 2 KB)");
    assert_eq!(lines[1], "  Root:         /w/proj");
}

#[test]
fn every_prune_rendering_names_its_root() {
    // The D5 property: a result is not readable without the root it applied to.
    for stats in [prune_stats(0, 0, 0), prune_stats(9, 1024, 512)] {
        let lines = format_prune(&stats, Path::new("/some/root"));
        assert!(lines.iter().any(|l| l.contains("/some/root")), "{lines:?}");
    }
}
