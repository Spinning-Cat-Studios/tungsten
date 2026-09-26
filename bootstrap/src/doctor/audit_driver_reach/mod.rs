//! Driver-reach census: which modules the driver runs, and which only a test
//! entry file reaches (ADR 3.9.26a).
//!
//! # Why this is not `audit-dead-definitions`
//!
//! That census is per **entry file** and per **definition**: run on
//! `src/compiler/main.tg` it reports definitions unreachable from that file's
//! roots, and run on `test_codegen.tg` it reports the whole codegen subsystem
//! as live. Both answers are true. Neither is "does the driver run this?",
//! which is a question about **modules** and spans **entry files** — the module
//! is test-only precisely because a *different* entry file is the only thing
//! that imports it.
//!
//! # Why the module tree cannot answer it either
//!
//! `src/compiler/main.tg` declares `mod codegen;`, so `tungsten info module
//! tree` lists the whole subsystem under the driver's root. A `mod`
//! declaration makes a module exist and be compiled; it does not make anything
//! call it. Reach here is therefore the **`use` graph**, with one structural
//! rule on top: reaching `a::b::c` reaches `a::b` and `a`, because a child
//! cannot be compiled without its parent.
//!
//! # Why parse and not elaborate
//!
//! The obvious implementation aggregates the termination gate's
//! `OccurrenceGraph` to module granularity, which is what ADR 3.9.26a's D3
//! proposed. Measured, that costs one full elaboration per entry file:
//! `audit-dead-definitions src/compiler/main.tg` takes 1 m 56 s on a debug
//! bootstrap, and `src/compiler/` has 22 entry files. The `use` graph is
//! parse-level, answers the same question at this granularity, and runs in
//! seconds — so this is cost 2, not the cost 3 the ADR planned for.
//!
//! # What it does not know
//!
//! That a `use` was written and the imported name never called. This over-
//! reports reach, deliberately: the failure worth catching is a subsystem
//! nothing imports at all, and a false *negative* there would be silent.

mod graph;
mod render;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub use graph::{module_graph, ModuleGraph};
pub use render::{render_reach, DriverReach};

/// The graph one run partitions: every module, its `use` edges, and the entry
/// files that root the walk.
///
/// Entry files are keys in `uses` alongside modules; they are the roots and are
/// never themselves classified, which is why `modules` is a separate set rather
/// than `uses.keys()`.
#[derive(Debug, Clone, Default)]
pub struct ReachInput {
    /// Every module that exists, by path relative to its entry file's root
    /// (`codegen::ir_types`), so the same module has the same key whichever
    /// entry file's tree it was found in.
    pub modules: BTreeSet<String>,
    /// Module-or-entry-file → the modules it imports directly.
    pub uses: BTreeMap<String, BTreeSet<String>>,
    /// The driver entry file's key in `uses`.
    pub driver_entry: String,
    /// The test entry files' keys in `uses`.
    pub test_entries: BTreeSet<String>,
    /// Entry files that were named but could not be parsed.
    pub unreadable_entries: BTreeSet<String>,
}

/// Whether a file stem names an entry file the `tungsten test` runner drives.
///
/// A free predicate rather than a closure inside discovery, so the convention
/// is assertable without a filesystem: `test_*` is the runner's own discovery
/// prefix, and `mustfail_*` is the must-fail twin convention (ADR 29.6.26f),
/// which is an entry file the sweep deliberately skips and which still imports
/// real modules.
#[must_use]
pub fn is_test_entry_stem(stem: &str) -> bool {
    // `test_`, not `test`: `tester.tg` would be an ordinary module.
    stem.starts_with("test_") || stem.starts_with("mustfail_")
}

/// The modules a qualified module path needs in order to exist.
///
/// `a::b::c` yields `a::b` and `a`. Reaching a child reaches its ancestors
/// because a submodule is only compiled through its parent's `mod`
/// declaration — without this rule every `mod.tg` in the tree reads as
/// unreached, since nothing ever imports a parent by name.
#[must_use]
pub fn ancestors_of(module: &str) -> Vec<String> {
    let segments: Vec<&str> = module.split("::").collect();
    (1..segments.len())
        .map(|end| segments[..end].join("::"))
        .collect()
}

/// Modules reachable from `root` through `uses`, ancestors included.
fn reachable_from(uses: &BTreeMap<String, BTreeSet<String>>, root: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut frontier: Vec<String> = uses.get(root).into_iter().flatten().cloned().collect();
    while let Some(module) = frontier.pop() {
        if !seen.insert(module.clone()) {
            continue;
        }
        frontier.extend(uses.get(&module).into_iter().flatten().cloned());
        frontier.extend(ancestors_of(&module));
    }
    seen
}

/// Partition every module into driver-reached, test-only and unreached.
///
/// Pure over `input`, so the whole decision is assertable against a hand-built
/// adjacency — including the shape `audit-dead-definitions` cannot express: a
/// module reachable from a test root and from no driver path.
#[must_use]
pub fn partition_reach(input: &ReachInput) -> DriverReach {
    let driver_reached: BTreeSet<String> = reachable_from(&input.uses, &input.driver_entry)
        .intersection(&input.modules)
        .cloned()
        .collect();

    let mut test_reached: BTreeSet<String> = BTreeSet::new();
    for entry in &input.test_entries {
        test_reached.extend(reachable_from(&input.uses, entry));
    }
    test_reached = test_reached.intersection(&input.modules).cloned().collect();

    DriverReach {
        examined: input.modules.len(),
        driver_entry: input.driver_entry.clone(),
        test_entries: input.test_entries.clone(),
        unreadable_entries: input.unreadable_entries.clone(),
        test_only: test_reached.difference(&driver_reached).cloned().collect(),
        unreached: input
            .modules
            .iter()
            .filter(|m| !driver_reached.contains(*m) && !test_reached.contains(*m))
            .cloned()
            .collect(),
        driver_reached,
    }
}

/// The test entry files sitting beside `driver_entry`, in sorted order.
///
/// Returns an empty vector rather than an error when the directory cannot be
/// read: the report says so on its own line, which is more useful than an exit
/// code nobody reads.
#[must_use]
pub fn discover_test_entries(driver_entry: &Path) -> Vec<PathBuf> {
    let Some(dir) = driver_entry.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "tg"))
        .filter(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(is_test_entry_stem)
        })
        .collect();
    found.sort();
    found
}

/// The test entry files for one run: the siblings discovery found, plus
/// `extra`, minus the driver entry itself, deduplicated and sorted.
///
/// Extracted from the command so the **exclusion** is assertable rather than
/// incidental. A driver entry that is also a test root would seed the test side
/// of the partition with the driver's own imports, and `test_only` would be
/// silently empty — the failure this whole report exists to make visible. It
/// happens two ways: `--test-entry` naming the driver file, and a driver entry
/// whose own stem is `test_*` (running the census on a suite).
#[must_use]
pub fn test_entries_for(driver_entry: &Path, extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut entries = discover_test_entries(driver_entry);
    entries.extend(extra.iter().cloned());
    entries.retain(|path| path != driver_entry);
    entries.sort();
    entries.dedup();
    entries
}

/// Parse every entry file and fold its module tree into one shared graph.
fn collect_input(driver_entry: &Path, test_entries: &[PathBuf]) -> Option<ReachInput> {
    let mut input = ReachInput {
        driver_entry: driver_entry.display().to_string(),
        ..ReachInput::default()
    };

    let driver_graph = module_graph(driver_entry)?;
    let driver_key = input.driver_entry.clone();
    absorb(&mut input, &driver_key, driver_graph);

    for entry in test_entries {
        let key = entry.display().to_string();
        match module_graph(entry) {
            Some(graph) => {
                input.test_entries.insert(key.clone());
                absorb(&mut input, &key, graph);
            }
            None => {
                input.unreadable_entries.insert(key);
            }
        }
    }
    Some(input)
}

/// Merge one entry file's module graph into the shared `ReachInput`.
fn absorb(input: &mut ReachInput, entry_key: &str, graph: ModuleGraph) {
    input.modules.extend(graph.modules);
    input
        .uses
        .entry(entry_key.to_string())
        .or_default()
        .extend(graph.entry_uses);
    for (module, used) in graph.uses {
        input.uses.entry(module).or_default().extend(used);
    }
}

/// Run the driver-reach census.
pub fn cmd_audit_driver_reach(file: &PathBuf, extra_test_entries: &[PathBuf]) -> ExitCode {
    let test_entries = test_entries_for(file, extra_test_entries);

    let Some(input) = collect_input(file, &test_entries) else {
        // The dangerous direction: a driver entry that could not be parsed must
        // not render as "0 modules examined, nothing test-only".
        eprintln!(
            "error: could not parse the module tree of {}",
            file.display()
        );
        return ExitCode::FAILURE;
    };

    print!("{}", render_reach(&partition_reach(&input)));

    // Reporting, never gating (ADR 3.9.26a Non-Goals): a module that is
    // test-only today and driver-reached tomorrow is progress, and a gate would
    // report it as a regression.
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests;
