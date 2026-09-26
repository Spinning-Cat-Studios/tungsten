//! Orphan-source census: the `.tg` files on disk that no module tree declares
//! (ADR 3.9.26q).
//!
//! # Why no existing tool answers this
//!
//! Every reachability tool in the repo starts from an entry file and walks
//! declarations — `tungsten check` parses the module tree, `audit-dead-
//! definitions` walks the elaborated call graph, `audit-driver-reach`
//! partitions the modules *in* the tree. A file that no `mod` statement names
//! is not in any of those starting sets, so none of them is structurally able
//! to report it. The comparison has to run the other way round: the filesystem
//! minus the tree (D1).
//!
//! `code-health` does read the filesystem, and does see these files — but only
//! for **size**. It counts their lines against directory and file budgets and
//! never asks whether anything reads them, so an orphan is simultaneously
//! scanned by the health gates and invisible to every question about whether it
//! does anything.
//!
//! # Why the remainder is three classes and not one
//!
//! `src/compiler/driver/ffi/diagnostics/dev.tg` is undeclared **by design**: it
//! is a variant that `make/native.mk` and `make/devcontainer.mk` copy over its
//! sibling `mod.tg` for developer builds. Reporting it beside the genuinely
//! stranded files would dilute the one real signal, and an allowlist would
//! record it as debt someone should pay down when it is not debt (D2). Entry
//! files are the third class for the same reason: `main.tg` and every
//! `test_*.tg` are roots, never modules, so they are always in the remainder
//! and mean nothing when they are.
//!
//! # Reports, never gates
//!
//! A file can be legitimately undeclared while it is being written, and a gate
//! would turn a work-in-progress module into a build break. Nor can the tool
//! know whether a stranded file should be deleted or wired up; that is a
//! judgement (D3).

mod render;

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub use render::{render_orphans, OrphanSources};

use super::audit_driver_reach::{is_test_entry_stem, test_entries_for};
use crate::driver::{parse_module_tree, ParsedModule};

/// The directories under the repository root a build recipe could plausibly
/// live in, and whose text therefore decides the **build-swapped** class.
///
/// A named list rather than "everything tracked": the census must be able to
/// say how many files it read, and a whole-repo grep would find a path
/// mentioned in a comment or an ADR and call it load-bearing.
const BUILD_SCAN_ROOTS: &[&str] = &["Makefile", "make", "scripts", ".github", "tools"];

/// File extensions that can carry a build recipe. `Makefile` itself is matched
/// by name, since it has none.
const BUILD_SCAN_EXTENSIONS: &[&str] = &["mk", "sh", "py", "toml", "yml", "yaml"];

/// Directories the build scan never descends into — build output and version
/// control, which hold no recipes and are large.
const BUILD_SCAN_SKIP_DIRS: &[&str] = &["target", ".git", "node_modules"];

/// What one census compares.
///
/// Every path is relative to the walk root (the entry file's directory), so
/// `parser/self_test.tg` is the same key however the run was invoked.
#[derive(Debug, Clone, Default)]
pub struct OrphanInput {
    /// Every `.tg` file under the walk root.
    pub files_on_disk: BTreeSet<String>,
    /// Every file some entry file's parsed module tree declares as a module.
    pub modules_in_tree: BTreeSet<String>,
    /// The entry files the walk rooted at — the driver entry plus its
    /// `test_*` / `mustfail_*` siblings.
    pub entry_files: BTreeSet<String>,
    /// Remainder paths a build file names (D2).
    pub build_mentions: BTreeSet<String>,
    /// Entry files that were named but could not be parsed. Named rather than
    /// dropped: a skipped entry file leaves its modules unsubtracted, which
    /// inflates `stranded` with files that are declared after all.
    pub unreadable_entries: BTreeSet<String>,
    /// How many build files the mention scan read. Zero means the
    /// build-swapped class is empty because nothing was compared against.
    pub build_files_scanned: usize,
}

/// Whether a file stem names an entry file rather than a module.
///
/// `main` is the driver root; `test_*` and `mustfail_*` are the `tungsten test`
/// runner's discovery conventions, shared with `audit-driver-reach` so the two
/// commands agree about what an entry file is. An entry file is never declared
/// by a `mod` statement, so it is always in the remainder and never a finding.
#[must_use]
pub fn is_entry_file_stem(stem: &str) -> bool {
    stem == "main" || is_test_entry_stem(stem)
}

/// The stem of a walk-root-relative path, or `""` when it has none.
fn stem_of(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".tg").unwrap_or(name)
}

/// Split the files on disk that no module tree declares into the three classes.
///
/// Pure over `input` — no filesystem access — so the whole decision is
/// assertable against a hand-built triple. The order matters: an entry file
/// that a `make` recipe also names is still an entry file, not a build-swapped
/// template.
#[must_use]
pub fn classify_orphans(input: &OrphanInput) -> OrphanSources {
    let mut sources = OrphanSources {
        files_walked: input.files_on_disk.len(),
        entry_files_read: input.entry_files.len(),
        modules_subtracted: input.modules_in_tree.len(),
        build_files_scanned: input.build_files_scanned,
        unreadable_entries: input.unreadable_entries.clone(),
        ..OrphanSources::default()
    };

    for path in input.files_on_disk.difference(&input.modules_in_tree) {
        if input.entry_files.contains(path) || is_entry_file_stem(stem_of(path)) {
            sources.entry_roots.insert(path.clone());
        } else if input.build_mentions.contains(path) {
            sources.build_swapped.insert(path.clone());
        } else {
            sources.stranded.insert(path.clone());
        }
    }
    sources
}

/// The remainder paths that appear verbatim in some build file's text.
///
/// Matching is on the walk-root-relative path (`driver/ffi/diagnostics/dev.tg`)
/// rather than the bare filename: a filename collides across directories, and a
/// recipe that copies a file always spells enough of its path to disambiguate.
#[must_use]
pub fn mentioned_in_build_files(
    candidates: &BTreeSet<String>,
    build_texts: &[String],
) -> BTreeSet<String> {
    candidates
        .iter()
        .filter(|path| build_texts.iter().any(|text| text.contains(path.as_str())))
        .cloned()
        .collect()
}

/// Every `.tg` file under `root`, as walk-root-relative paths.
///
/// Hidden directories are skipped: `.tungsten` elaboration caches sit beside
/// source and are not source.
#[must_use]
pub fn walk_tg_files(root: &Path) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut frontier = vec![root.to_path_buf()];
    while let Some(dir) = frontier.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let is_hidden = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'));
            if is_hidden {
                continue;
            }
            if path.is_dir() {
                frontier.push(path);
            } else if path.extension().is_some_and(|ext| ext == "tg") {
                found.insert(relative_key(root, &path));
            }
        }
    }
    found
}

/// `path` relative to `root`, with forward slashes.
///
/// A path that is not under `root` keeps its own spelling rather than being
/// dropped: the one case that reaches it is an entry file named without a
/// directory (`main.tg` against a root of `.`), and dropping that key would
/// classify the driver's own entry file as stranded.
fn relative_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The source files one entry file's parsed module tree declares.
///
/// The entry file itself is excluded — it is a root, not a module — which is
/// what puts every entry file in the remainder and makes the third class
/// necessary.
fn declared_module_files(entry: &Path, root: &Path) -> Option<BTreeSet<String>> {
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(entry, &mut visited, &mut chain, None).ok()?;
    let mut declared = BTreeSet::new();
    collect_module_paths(&tree, root, &mut declared);
    Some(declared)
}

/// Record every submodule's source path, recursively.
fn collect_module_paths(module: &ParsedModule, root: &Path, out: &mut BTreeSet<String>) {
    for child in &module.submodules {
        out.insert(relative_key(root, &child.path));
        collect_module_paths(child, root, out);
    }
}

/// The nearest ancestor of `start` holding a `Makefile` — the repository root
/// whose build files decide the build-swapped class.
#[must_use]
pub fn repo_root_of(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join("Makefile").is_file())
        .map(Path::to_path_buf)
}

/// Read every build file under `repo_root`'s scan roots.
#[must_use]
pub fn build_file_texts(repo_root: &Path) -> Vec<String> {
    let mut texts = Vec::new();
    for root in BUILD_SCAN_ROOTS {
        read_build_files(&repo_root.join(root), &mut texts);
    }
    texts
}

/// Read one scan root — a file, or a directory walked recursively.
fn read_build_files(path: &Path, out: &mut Vec<String>) {
    if path.is_file() {
        if is_build_file(path) {
            if let Ok(text) = std::fs::read_to_string(path) {
                out.push(text);
            }
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let child = entry.path();
        let skipped = child
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| BUILD_SCAN_SKIP_DIRS.contains(&n));
        if !skipped {
            read_build_files(&child, out);
        }
    }
}

/// Whether a file can carry a build recipe.
#[must_use]
pub fn is_build_file(path: &Path) -> bool {
    let named_makefile = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == "Makefile");
    let eligible_extension = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| BUILD_SCAN_EXTENSIONS.contains(&ext));
    named_makefile || eligible_extension
}

/// The directory a census rooted at `entry` walks.
///
/// A bare filename's parent is the **empty** path, which no directory read
/// resolves: the walk would then find nothing and the census would render as
/// clean rather than as unrun. `.` is what the caller meant.
#[must_use]
pub fn walk_root_of(entry: &Path) -> Option<&Path> {
    match entry.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Some(Path::new(".")),
        other => other,
    }
}

/// Walk the disk, parse every entry file's tree, and scan the build files.
fn collect_input(entry: &Path) -> Option<OrphanInput> {
    let root = walk_root_of(entry)?;
    let entries: Vec<PathBuf> = std::iter::once(entry.to_path_buf())
        .chain(test_entries_for(entry, &[]))
        .collect();

    let mut input = OrphanInput {
        files_on_disk: walk_tg_files(root),
        ..OrphanInput::default()
    };

    // The driver entry must parse: a run that subtracted nothing would report
    // the whole tree as stranded.
    input
        .modules_in_tree
        .extend(declared_module_files(entry, root)?);
    input.entry_files.insert(relative_key(root, entry));

    for other in entries.iter().skip(1) {
        let key = relative_key(root, other);
        match declared_module_files(other, root) {
            Some(declared) => {
                input.entry_files.insert(key);
                input.modules_in_tree.extend(declared);
            }
            None => {
                input.unreadable_entries.insert(key);
            }
        }
    }

    let remainder: BTreeSet<String> = input
        .files_on_disk
        .difference(&input.modules_in_tree)
        .cloned()
        .collect();
    if let Some(repo_root) = repo_root_of(root) {
        let texts = build_file_texts(&repo_root);
        input.build_files_scanned = texts.len();
        input.build_mentions = mentioned_in_build_files(&remainder, &texts);
    }

    Some(input)
}

/// Run the orphan-source census.
pub fn cmd_audit_orphan_sources(file: &PathBuf) -> ExitCode {
    let Some(input) = collect_input(file) else {
        // The dangerous direction: an entry file that could not be parsed must
        // not render as "everything on disk is stranded".
        eprintln!(
            "error: could not parse the module tree of {}",
            file.display()
        );
        return ExitCode::FAILURE;
    };

    print!("{}", render_orphans(&classify_orphans(&input)));

    // Reporting, never gating (D3).
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests;
