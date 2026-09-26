//! What a walk found, and the test-reference index a report needs.
//!
//! Data and predicates only — no traversal, no rendering, no I/O — so every
//! verdict in this file is assertable over hand-built values.

use std::collections::{BTreeMap, BTreeSet};

use tungsten_core::Term;

use super::walk::collect_refs;

/// One extern reached from the root definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReachedExtern {
    /// The symbol the evaluator dispatches on, `__c_` already stripped.
    pub symbol: String,
    /// Whether the evaluator has an arm for it. `false` means any call on this
    /// path goes silently `Stuck`.
    pub executable: bool,
    /// Shortest chain of definitions from the root to the one that calls it,
    /// root first. A one-element chain means the root calls it directly.
    pub via: Vec<String>,
}

/// What a walk from one definition found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reachability {
    /// The definition the walk started from.
    pub root: String,
    /// Definitions actually walked, the root included — so `1` says "the root's
    /// body referenced nothing", which must never render the same as "nothing
    /// was examined".
    pub defs_visited: usize,
    /// Globals referenced that have no definition in the project. Reported
    /// rather than ignored: an unresolved global is a hole in this walk, so a
    /// clean verdict beside a non-empty list is only clean as far as it got.
    pub unresolved: Vec<String>,
    /// Definitions the walk found but refused to enter because the visit budget
    /// was exhausted, alphabetical. Non-empty is the **third** terminal state
    /// beside clean and blocked: the answer is partial, and every count above
    /// is a lower bound. A truncated list that renders like a complete one is
    /// worse than no answer, so this is reported, never merely implied.
    pub not_reached: Vec<String>,
    /// Every extern reached, unexecutable first, then alphabetical.
    pub reached: Vec<ReachedExtern>,
    /// `test_*` definitions referencing the root **directly**, alphabetical
    /// (`TestReferences` says why direct). Empty is the interesting value,
    /// and only when nothing blocks: executable-and-unasserted is coverage
    /// going unused, which ADR 19.8.26c shipped and only review caught.
    pub reached_by_tests: Vec<String>,
}

impl Reachability {
    /// The externs that would go `Stuck` — the finding this command exists for.
    pub fn blocking(&self) -> impl Iterator<Item = &ReachedExtern> {
        self.reached.iter().filter(|e| !e.executable)
    }

    /// Whether the walk exhausted the graph rather than its budget.
    ///
    /// Every "nothing found" claim downstream is gated on this: a clean verdict
    /// over a partial walk is the believable wrong answer ADR 3.9.26c's D2
    /// exists to prevent.
    pub fn complete(&self) -> bool {
        self.not_reached.is_empty()
    }

    /// Executable end to end, yet no `test_*` calls it. False for a `test_*`
    /// root itself — flagging a test as untested is noise where the reader is
    /// already looking at one — and false for a partial walk, whose "nothing
    /// blocks" is only true as far as it got.
    pub fn assertable_but_untested(&self) -> bool {
        self.complete()
            && self.blocking().next().is_none()
            && self.reached_by_tests.is_empty()
            && !is_test_definition(&self.root)
    }
}

/// Whether a global's name marks it as a `tungsten test` entry point.
///
/// On the **last path segment**, so `driver::x::test_foo` counts and
/// `contest_foo` does not. A hint, not a gate: it does not re-derive the
/// runner's zero-params / `Unit`-return rules.
pub(crate) fn is_test_definition(name: &str) -> bool {
    name.rsplit("::")
        .next()
        .unwrap_or(name)
        .starts_with("test_")
}

/// Which `test_*` definitions name which globals, built once per elaboration.
///
/// **DIRECT references only** — transitive was measured and rejected: one broad
/// suite reaches most of the driver, making the flag fire nowhere. Worked, with
/// the measurement, in `diagnostic-tools-cheatsheet.md` under this command.
///
/// Indexed rather than recomputed because the multi-root form (ADR 3.9.26c D1)
/// answers about N definitions from one elaboration: scanning every `test_*`
/// body once per root would put back a per-root cost the whole change exists to
/// remove.
pub(crate) struct TestReferences {
    /// `(test definition, the globals its own body names)`, in name order — so
    /// `referencing` yields alphabetically without a second sort.
    entries: Vec<(String, BTreeSet<String>)>,
}

impl TestReferences {
    /// Scan every `test_*` definition's body once.
    pub fn index(globals: &BTreeMap<String, Term>) -> Self {
        let entries = globals
            .iter()
            .filter(|(name, _)| is_test_definition(name))
            .map(|(name, body)| {
                let mut refs = super::walk::Refs::default();
                collect_refs(body, &mut refs);
                (name.clone(), refs.globals.into_iter().collect())
            })
            .collect();
        Self { entries }
    }

    /// Every `test_*` definition whose own body names `target`, alphabetical.
    pub fn referencing(&self, target: &str) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(name, refs)| name.as_str() != target && refs.contains(target))
            .map(|(name, _)| name.clone())
            .collect()
    }
}
