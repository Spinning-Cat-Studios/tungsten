//! The Core call-graph traversal, and the budget that bounds it.
//!
//! PURE — a `BTreeMap` of definition bodies in, a verdict out. No elaboration,
//! no file, no registry lookup beyond the compile-time table.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use tungsten_core::eval::extern_registry;
use tungsten_core::Term;

use super::report::{Reachability, ReachedExtern, TestReferences};

/// Definitions one walk may enter before it must report a partial answer.
///
/// Sized well above any single entry file's definition count (`main.tg` holds
/// ~2300, and a walk can visit each at most once), so the default cannot change
/// an answer that completes today — ADR 3.9.26c AC4. It exists for the case
/// §1.3 measured, where a walk returned nothing at all inside a ten-minute
/// probe and the operator could not tell slow from wedged.
pub const DEFAULT_MAX_VISITED: usize = 10_000;

/// Strip the `__c_` C-ABI prefix the elaborator prepends to extern symbols.
///
/// Mirrors `step_extern_call_env` (`eval/env/handlers/externs/call.rs`). If
/// these two ever disagree, this command reports executability for a symbol the
/// evaluator never looks up.
fn dispatch_symbol(raw: &str) -> &str {
    raw.strip_prefix("__c_").unwrap_or(raw)
}

/// References one term makes, without recursing into other definitions.
#[derive(Default)]
pub(crate) struct Refs {
    pub externs: Vec<String>,
    pub globals: Vec<String>,
}

/// Collect every `Global` and `ExternCall` in `term`.
///
/// Drives recursion through `Term::for_each_subterm`, which enumerates every
/// variant explicitly, rather than a local `match` with a `_ => {}` arm: an
/// unhandled variant here would under-walk the term and report a **false
/// clean**, which is the exact failure this command exists to catch.
pub(crate) fn collect_refs(term: &Term, out: &mut Refs) {
    match term {
        Term::Global(name) => out.globals.push(name.clone()),
        Term::ExternCall(symbol, _) => out.externs.push(dispatch_symbol(symbol).to_string()),
        _ => {}
    }
    term.for_each_subterm(|child| collect_refs(child, out));
}

/// Walk the Core call graph from `root`, collecting the externs it reaches.
///
/// Breadth-first, so the first sighting of a symbol carries the **shortest**
/// call chain that reaches it — which is the part that makes the report
/// actionable rather than merely alarming.
///
/// `max_visited` caps definitions entered. Hitting it does not truncate the
/// report: the definitions refused entry are recorded in `not_reached`, and the
/// verdict becomes *incomplete* rather than a shorter clean one.
///
/// `tests` is the shared index, built once per elaboration, so N roots pay for
/// one scan of the `test_*` bodies rather than N.
///
/// Returns `None` when `root` is not a definition in `globals`.
pub(crate) fn analyze(
    globals: &BTreeMap<String, Term>,
    root: &str,
    max_visited: usize,
    tests: &TestReferences,
) -> Option<Reachability> {
    let root_term = globals.get(root)?;
    let reached_by_tests = tests.referencing(root);

    if max_visited == 0 {
        // Nothing was examined — including the root. Reported as a partial walk
        // so it can never read as "walked it, found nothing".
        return Some(Reachability {
            root: root.to_string(),
            defs_visited: 0,
            unresolved: Vec::new(),
            not_reached: vec![root.to_string()],
            reached: Vec::new(),
            reached_by_tests,
        });
    }

    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut unresolved: BTreeSet<String> = BTreeSet::new();
    let mut not_reached: BTreeSet<String> = BTreeSet::new();
    let mut seen_symbols: BTreeSet<String> = BTreeSet::new();
    let mut reached: Vec<ReachedExtern> = Vec::new();
    let mut queue: VecDeque<(&str, &Term, Vec<String>)> = VecDeque::new();

    visited.insert(root);
    queue.push_back((root, root_term, vec![root.to_string()]));

    while let Some((_name, term, path)) = queue.pop_front() {
        let mut refs = Refs::default();
        collect_refs(term, &mut refs);

        for symbol in refs.externs {
            if seen_symbols.insert(symbol.clone()) {
                reached.push(ReachedExtern {
                    executable: extern_registry::is_executable(&symbol),
                    symbol,
                    via: path.clone(),
                });
            }
        }

        for callee in refs.globals {
            match globals.get_key_value(callee.as_str()) {
                Some((name, body)) => {
                    if visited.contains(name.as_str()) {
                        continue;
                    }
                    if visited.len() >= max_visited {
                        // Refused, not dropped: the budget's whole point is that
                        // the report can name what it did not look at.
                        not_reached.insert(name.clone());
                        continue;
                    }
                    visited.insert(name.as_str());
                    let mut next = path.clone();
                    next.push(name.clone());
                    queue.push_back((name.as_str(), body, next));
                }
                None => {
                    unresolved.insert(callee);
                }
            }
        }
    }

    // Unexecutable first — the reader is here for those — then alphabetical so
    // two runs of the same file diff cleanly.
    reached.sort_by(|a, b| {
        a.executable
            .cmp(&b.executable)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });

    Some(Reachability {
        root: root.to_string(),
        defs_visited: visited.len(),
        unresolved: unresolved.into_iter().collect(),
        not_reached: not_reached.into_iter().collect(),
        reached,
        reached_by_tests,
    })
}
