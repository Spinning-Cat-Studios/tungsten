//! The SCC edge collector (ADR 7.8.26e D3).
//!
//! **The invariant, not an implementation note:** this traversal must visit a
//! *superset* of what [`super::walker::Walk`] visits. An edge the graph misses
//! splits an SCC, and a split SCC is a false accept — the exact unsoundness the
//! gate exists to close. Two ways it stays a superset:
//!
//! - Arguments are walked unconditionally, ignoring the three-way `param_occ`
//!   dispatch (which may *skip* an `Unused` argument).
//! - An `Adt`'s variant payloads are always walked (the walker skips them when
//!   the head is in `defs`).
//!
//! And two things it must do that the elaborator's existing `collect_type_refs`
//! does not:
//!
//! - Descend into `Eq`'s witness terms. `Type::children()` yields only the type
//!   argument and, by documented design, never the embedded types — so reusing
//!   it makes a cycle whose only edge runs through a witness invisible.
//! - Filter `Forall`/`Mu` binders and enclosing type parameters. A binder that
//!   shadows a definition name would otherwise record a spurious edge, hence an
//!   over-wide SCC — a false-rejection vector, and there is no escape hatch.

use std::collections::{BTreeMap, BTreeSet};

use crate::terms::Term;
use crate::types::Type;

use super::defs::PositivityDefs;

/// `App`/`Adt` heads with no entry in `defs`, split by why (ADR 7.8.26e D6).
///
/// The two have different fixes: a `Stub` head is an import whose real body has
/// not been collected in this pass, whereas a genuinely-absent head is a
/// primitive or an import that did not travel at all — and only the latter
/// makes D5's doubt arm fire.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeadCensus {
    /// Heads naming a lossy `TypeDefKind::Stub` — skipped, not doubted.
    pub stub: BTreeSet<String>,
    /// Heads absent from `defs` entirely — D5's doubt arm, args forbidden.
    pub unknown: BTreeSet<String>,
}

/// Adjacency for the type dependency graph: definition name → the definition
/// names it references, restricted to nodes of `defs`.
///
/// Every definition in `defs` gets an entry, so a caller can build the node set
/// from the keys alone.
#[must_use]
pub fn referenced_names(defs: &PositivityDefs) -> BTreeMap<String, BTreeSet<String>> {
    walk_all(defs).0
}

/// The `App`/`Adt` heads this corpus references but cannot resolve.
#[must_use]
pub fn head_census(defs: &PositivityDefs) -> HeadCensus {
    walk_all(defs).1
}

/// One traversal of every definition, producing both the adjacency and the
/// unresolved-head census.
fn walk_all(defs: &PositivityDefs) -> (BTreeMap<String, BTreeSet<String>>, HeadCensus) {
    let nodes: BTreeSet<String> = defs.names().cloned().collect();
    let mut census = HeadCensus::default();
    let adjacency = defs
        .iter()
        .map(|(name, def)| {
            let mut collector = RefCollector {
                nodes: &nodes,
                defs,
                params: &def.params,
                bound: Vec::new(),
                out: BTreeSet::new(),
                census: &mut census,
            };
            for ctor in &def.ctors {
                for (_, ty) in &ctor.fields {
                    collector.collect(ty);
                }
            }
            (name.clone(), collector.out)
        })
        .collect();
    (adjacency, census)
}

struct RefCollector<'a> {
    nodes: &'a BTreeSet<String>,
    defs: &'a PositivityDefs,
    params: &'a [String],
    bound: Vec<String>,
    out: BTreeSet<String>,
    census: &'a mut HeadCensus,
}

impl RefCollector<'_> {
    fn record(&mut self, raw: &str) {
        let name = raw.strip_prefix('@').unwrap_or(raw);
        if self.nodes.contains(name) {
            self.out.insert(name.to_string());
        }
    }

    /// Record an `App`/`Adt` head, censusing it when it does not resolve.
    ///
    /// `TyVar` heads are deliberately excluded from the census: they carry no
    /// arguments, so D5's doubt arm never fires on one.
    fn record_head(&mut self, raw: &str) {
        let name = raw.strip_prefix('@').unwrap_or(raw);
        if self.nodes.contains(name) {
            self.out.insert(name.to_string());
        } else if self.defs.is_stub(name) {
            self.census.stub.insert(name.to_string());
        } else {
            self.census.unknown.insert(name.to_string());
        }
    }

    fn collect(&mut self, ty: &Type) {
        match ty {
            Type::TyVar(raw) => {
                let name = raw.strip_prefix('@').unwrap_or(raw);
                // Roles (b) and (c): bound variables and enclosing parameters
                // are not references to a definition, even when they shadow one.
                if self.bound.iter().any(|b| b == raw || b == name)
                    || self.params.iter().any(|p| p == name)
                {
                    return;
                }
                self.record(name);
            }
            Type::Forall(binder, body) | Type::Mu(binder, body) => {
                self.bound.push(binder.clone());
                self.collect(body);
                self.bound.pop();
            }
            Type::App(name, args) => {
                self.record_head(name);
                for arg in args {
                    self.collect(arg);
                }
            }
            Type::Adt(name, args, variants) => {
                self.record_head(name);
                for arg in args {
                    self.collect(arg);
                }
                for (_, payload) in variants {
                    self.collect(payload);
                }
            }
            Type::Eq(ty_arg, left, right) => {
                self.collect(ty_arg);
                self.collect_term(left);
                self.collect_term(right);
            }
            // Arrow / Product / Sum / Ptr / Ref and the leaves: no edge of
            // their own, recurse structurally.
            _ => {
                for child in ty.children() {
                    self.collect(child);
                }
            }
        }
    }

    fn collect_term(&mut self, term: &Term) {
        term.for_each_embedded_type(|ty| self.collect(ty));
        term.for_each_subterm(|sub| self.collect_term(sub));
    }
}
