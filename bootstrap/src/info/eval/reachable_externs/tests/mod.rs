//! Tests for `tungsten info eval reachable-externs`.
//!
//! Every case here is a hand-built `Term` graph, per the "pure function over
//! injected data" idiom: the walk touches no file, no elaborator and no
//! evaluator, so the whole decision is assertable in microseconds.
//!
//! `tg_println` and `tg_path_join` are used as the executable / unexecutable
//! representatives. They are real entries (and non-entries) in
//! `EXECUTABLE_EXTERNS`, so a change to that registry that broke this command's
//! premise fails these tests rather than silently changing their meaning —
//! which is the point of not stubbing the predicate.

mod budget;
mod graph;
mod multi_root;
mod render;
mod test_hints;

use std::collections::BTreeMap;

use tungsten_core::Term;

use super::report::{Reachability, TestReferences};
use super::walk::{analyze, DEFAULT_MAX_VISITED};

/// `λ_. ExternCall(symbol)` — the shape an `extern "C" fn` declaration
/// elaborates to.
fn extern_wrapper(symbol: &str) -> Term {
    Term::ExternCall(symbol.to_string(), vec![Term::Unit])
}

/// `App(Global(callee), Unit)` — one definition calling another.
fn calls(callee: &str) -> Term {
    Term::App(
        Box::new(Term::Global(callee.to_string())),
        Box::new(Term::Unit),
    )
}

fn project(defs: &[(&str, Term)]) -> BTreeMap<String, Term> {
    defs.iter()
        .map(|(n, t)| ((*n).to_string(), t.clone()))
        .collect()
}

/// The walk as the single-definition CLI form runs it: default budget, an index
/// built for this call.
fn analyze_default(globals: &BTreeMap<String, Term>, root: &str) -> Option<Reachability> {
    analyze(
        globals,
        root,
        DEFAULT_MAX_VISITED,
        &TestReferences::index(globals),
    )
}

/// The walk under an explicit budget.
fn analyze_bounded(
    globals: &BTreeMap<String, Term>,
    root: &str,
    max_visited: usize,
) -> Option<Reachability> {
    analyze(globals, root, max_visited, &TestReferences::index(globals))
}
