//! Tests for the orphan-source census, split by what they inject.
//!
//! [`classification`] and [`rendering`] run entirely over hand-built inputs —
//! no filesystem — because the three-way split and the report are the two
//! things a reader has to be able to argue with (AC 2). [`fixtures`] is the
//! other half: real `.tg` trees and a real `Makefile` under a `TempDir`, the
//! only way to assert that the walk and the subtraction agree with the parser.
//!
//! The coupling none of those can see — the recipe text that makes a file
//! build-swapped rather than stranded still has to name a file that exists — is
//! **no longer guarded here**. ADR 3.9.26t added a `build_swap` module holding a
//! prototype extractor and an assertion about the one `diagnostics/dev.tg` pair;
//! ADR 5.9.26e generalised both into the `recipe-inputs` code-health check,
//! which asks the same question of every `cp` in the tracked build corpus and
//! **gates** on the answer. Re-adding a copy here would be a second extractor
//! free to disagree with the one that runs.
//!
//! `TempDir` throughout, deliberately: it releases on drop (ADR 18.8.26d).

mod classification;
mod fixtures;
mod rendering;

use std::fs;

use tempfile::TempDir;

use super::*;

/// A `BTreeSet<String>` from string literals.
fn set(of: &[&str]) -> BTreeSet<String> {
    of.iter().map(|n| (*n).to_string()).collect()
}

/// An `OrphanInput` from literals, with one build file read.
fn input(
    files_on_disk: &[&str],
    modules_in_tree: &[&str],
    entry_files: &[&str],
    build_mentions: &[&str],
) -> OrphanInput {
    OrphanInput {
        files_on_disk: set(files_on_disk),
        modules_in_tree: set(modules_in_tree),
        entry_files: set(entry_files),
        build_mentions: set(build_mentions),
        unreadable_entries: BTreeSet::new(),
        build_files_scanned: 1,
    }
}

/// A tree with one of each class: `engine/core.tg` declared, `engine/spare.tg`
/// stranded, `engine/core_dev.tg` copied over `core.tg` by the `Makefile`, and
/// two entry files.
fn fixture_project() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    fs::write(
        root.join("Makefile"),
        "dev:\n\t@cp project/engine/core_dev.tg project/engine/core.tg\n",
    )
    .expect("write");

    let project = root.join("project");
    fs::create_dir(&project).expect("mkdir");
    fs::create_dir(project.join("engine")).expect("mkdir");
    fs::write(project.join("engine/mod.tg"), "pub mod core;\n").expect("write");
    fs::write(
        project.join("engine/core.tg"),
        "pub fn core_value() -> Nat { 1 }\n",
    )
    .expect("write");
    fs::write(
        project.join("engine/core_dev.tg"),
        "pub fn core_value() -> Nat { 2 }\n",
    )
    .expect("write");
    fs::write(
        project.join("engine/spare.tg"),
        "pub fn unused() -> Nat { 3 }\n",
    )
    .expect("write");
    fs::write(
        project.join("main.tg"),
        "mod engine;\nuse engine::core::core_value;\npub fn main() -> Nat { core_value() }\n",
    )
    .expect("write");
    fs::write(
        project.join("test_core.tg"),
        "mod engine;\nuse engine::core::core_value;\npub fn test_core() -> Nat { core_value() }\n",
    )
    .expect("write");
    dir
}
