//! Tests for the driver-reach census, split by what they inject.
//!
//! [`partition`] and [`rendering`] run entirely over hand-built adjacencies —
//! no filesystem, no parser — because the partition and the report are the two
//! things a reader has to be able to argue with. [`fixtures`] is the other half:
//! real `.tg` trees under a `TempDir`, which is the only way to assert that the
//! extraction agrees with the parser.
//!
//! `TempDir` throughout, deliberately: it releases on drop (ADR 18.8.26d).

mod fixtures;
mod partition;
mod rendering;

use std::fs;

use tempfile::TempDir;

use super::*;

/// A `BTreeSet<String>` from string literals.
fn set(of: &[&str]) -> BTreeSet<String> {
    of.iter().map(|n| (*n).to_string()).collect()
}

/// Build a `ReachInput` from an adjacency literal.
fn input(
    modules: &[&str],
    uses: &[(&str, &[&str])],
    driver_entry: &str,
    test_entries: &[&str],
) -> ReachInput {
    ReachInput {
        modules: set(modules),
        uses: uses
            .iter()
            .map(|(from, to)| ((*from).to_string(), set(to)))
            .collect(),
        driver_entry: driver_entry.to_string(),
        test_entries: set(test_entries),
        unreadable_entries: BTreeSet::new(),
    }
}

/// Partition an adjacency literal and render the result.
fn rendered(modules: &[&str], uses: &[(&str, &[&str])], test_entries: &[&str]) -> String {
    render_reach(&partition_reach(&input(
        modules,
        uses,
        "main.tg",
        test_entries,
    )))
}

/// A two-module tree plus the entry files that import it: `engine::core` is
/// driver-reached, `engine::emitter` is reached only from the test entry.
fn fixture_project() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    fs::create_dir(root.join("engine")).expect("mkdir");
    fs::write(
        root.join("engine/mod.tg"),
        "pub mod core;\npub mod emitter;\n",
    )
    .expect("write");
    fs::write(
        root.join("engine/core.tg"),
        "pub fn core_value() -> Nat { 1 }\n",
    )
    .expect("write");
    fs::write(
        root.join("engine/emitter.tg"),
        "pub fn emit() -> Nat { 2 }\n",
    )
    .expect("write");
    fs::write(
        root.join("main.tg"),
        "mod engine;\nuse engine::core::core_value;\npub fn main() -> Nat { core_value() }\n",
    )
    .expect("write");
    fs::write(
        root.join("test_emitter.tg"),
        "mod engine;\nuse engine::emitter::emit;\npub fn test_emit() -> Nat { emit() }\n",
    )
    .expect("write");
    dir
}
