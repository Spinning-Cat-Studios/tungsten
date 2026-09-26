//! The occurrence graph: which definitions each definition mentions, split by
//! whether the mention is a **call** or an opaque value occurrence
//! (ADR 29.6.26e § No indirect recursion escape hatch).
//!
//! The split is the whole point. A direct-call-only scan is a soundness hole —
//! `let f = diverge; f()` recurses without a single call-position occurrence of
//! `diverge` — so the graph records both edge kinds and the checker rejects an
//! intra-SCC occurrence that is not a call it can inspect.

use std::collections::{BTreeMap, BTreeSet};

use crate::terms::Term;

/// Caller → callee edges, split by occurrence kind.
///
/// SCCs are computed over the **union** of both edge sets: an indirect cycle is
/// still a cycle, and it must be found before it can be rejected.
#[derive(Debug, Default)]
pub struct OccurrenceGraph {
    nodes: BTreeSet<String>,
    call_edges: BTreeMap<String, BTreeSet<String>>,
    opaque_edges: BTreeMap<String, BTreeSet<String>>,
}

impl OccurrenceGraph {
    /// Build the graph from definition name → body term.
    ///
    /// Only globals that name a known definition become edges; an unresolved
    /// `Global` (an extern, a builtin) is not a node.
    #[must_use]
    pub fn build<'a>(defs: impl Iterator<Item = (&'a str, &'a Term)>) -> Self {
        Self::build_knowing(defs, &BTreeSet::new())
    }

    /// [`OccurrenceGraph::build`], also recognising `also_known` as definition
    /// names when classifying occurrences.
    ///
    /// The extra names become *edge targets* but not *nodes*: they are
    /// definitions restored from a cache, so they have no term to walk and no
    /// place in an SCC, but a fresh definition mentioning one is still a
    /// mention and taint has to see it.
    #[must_use]
    pub fn build_knowing<'a>(
        defs: impl Iterator<Item = (&'a str, &'a Term)>,
        also_known: &BTreeSet<String>,
    ) -> Self {
        let entries: Vec<(&str, &Term)> = defs.collect();
        let nodes: BTreeSet<String> = entries
            .iter()
            .map(|(name, _)| (*name).to_string())
            .collect();
        let recognised: BTreeSet<String> = nodes.union(also_known).cloned().collect();

        let mut graph = OccurrenceGraph {
            call_edges: BTreeMap::new(),
            opaque_edges: BTreeMap::new(),
            nodes,
        };
        for (name, term) in entries {
            let mut calls = BTreeSet::new();
            let mut opaque = BTreeSet::new();
            collect_occurrences(term, &recognised, &mut calls, &mut opaque);
            graph.call_edges.insert(name.to_string(), calls);
            graph.opaque_edges.insert(name.to_string(), opaque);
        }
        graph
    }

    /// Every definition name in the graph.
    #[must_use]
    pub fn nodes(&self) -> &BTreeSet<String> {
        &self.nodes
    }

    /// Callees of `name` reached in call position.
    #[must_use]
    pub fn calls(&self, name: &str) -> Option<&BTreeSet<String>> {
        self.call_edges.get(name)
    }

    /// Globals `name` mentions somewhere other than call position.
    #[must_use]
    pub fn opaque_uses(&self, name: &str) -> Option<&BTreeSet<String>> {
        self.opaque_edges.get(name)
    }

    /// Everything `name` mentions, either way — the adjacency SCCs use.
    #[must_use]
    pub fn mentions(&self, name: &str) -> BTreeSet<String> {
        let mut all = self.call_edges.get(name).cloned().unwrap_or_default();
        if let Some(opaque) = self.opaque_edges.get(name) {
            all.extend(opaque.iter().cloned());
        }
        all
    }

    /// Whether `from` mentions `to` in any position.
    #[must_use]
    pub fn has_edge(&self, from: &str, to: &str) -> bool {
        self.call_edges
            .get(from)
            .is_some_and(|set| set.contains(to))
            || self
                .opaque_edges
                .get(from)
                .is_some_and(|set| set.contains(to))
    }

    /// Number of definitions.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of caller → callee pairs, counting a name reached both ways once.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.nodes.iter().map(|n| self.mentions(n).len()).sum()
    }

    /// Adjacency over the union of both edge sets, for SCC computation.
    ///
    /// Keyed on nodes only, so an edge to a cache-carried name is present but
    /// leads nowhere — which is what `tarjan_scc` already skips.
    #[must_use]
    pub fn adjacency(&self) -> BTreeMap<String, BTreeSet<String>> {
        self.nodes
            .iter()
            .map(|name| (name.clone(), self.mentions(name)))
            .collect()
    }
}

/// Strip the wrappers that carry no semantics, so pattern matches downstream do
/// not each have to spell them out.
#[must_use]
pub fn transparent(term: &Term) -> &Term {
    match term {
        Term::Spanned(inner, _) | Term::Annot(inner, _) => transparent(inner),
        other => other,
    }
}

/// Peel an application spine into its head and value arguments, left to right.
///
/// `TyApp` is peeled too but contributes no value argument — a type application
/// does not consume a parameter, so `f[T](x)` supplies `x` at position 0.
#[must_use]
pub fn peel_spine(term: &Term) -> (&Term, Vec<&Term>) {
    match transparent(term) {
        Term::App(head, arg) => {
            let (root, mut args) = peel_spine(head);
            args.push(arg);
            (root, args)
        }
        Term::TyApp(head, _) => peel_spine(head),
        other => (other, Vec::new()),
    }
}

/// Record a spine head: a call when it names a known definition and the spine
/// supplies at least one argument, an ordinary sub-term otherwise.
fn collect_spine_head(
    head: &Term,
    args: &[&Term],
    known: &BTreeSet<String>,
    calls: &mut BTreeSet<String>,
    opaque: &mut BTreeSet<String>,
) {
    match head {
        Term::Global(name) if known.contains(name) && !args.is_empty() => {
            calls.insert(name.clone());
        }
        other => collect_occurrences(other, known, calls, opaque),
    }
}

/// Record every known-global occurrence in `term`, split by position.
fn collect_occurrences(
    term: &Term,
    known: &BTreeSet<String>,
    calls: &mut BTreeSet<String>,
    opaque: &mut BTreeSet<String>,
) {
    let stripped = transparent(term);
    match stripped {
        Term::Global(name) => {
            if known.contains(name) {
                opaque.insert(name.clone());
            }
        }
        Term::App(..) | Term::TyApp(..) => {
            let (head, args) = peel_spine(stripped);
            collect_spine_head(head, &args, known, calls, opaque);
            for arg in args {
                collect_occurrences(arg, known, calls, opaque);
            }
        }
        other => other.for_each_subterm(|child| collect_occurrences(child, known, calls, opaque)),
    }
}
