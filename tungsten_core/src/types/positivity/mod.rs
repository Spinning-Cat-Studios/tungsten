//! Strict-positivity analysis over the elaborated `Type` (ADR 7.8.26e).
//!
//! A strictly-positive inductive is one whose recursive occurrences never
//! appear to the left of an arrow **at any depth** — not "an even number of
//! times", which is the weaker *positivity* condition. The restriction is what
//! makes "structural subterm" a well-founded relation, and therefore what a
//! termination checker (F5, ADR 29.6.26e) rests on: without it
//! `type Bad = Mk(Bad -> Bad)` yields a diverging term at any type with no
//! syntactic recursion for a termination checker to inspect, and at
//! `type Bad = Mk(Bad -> Void)` a closed inhabitant of the empty type.
//!
//! The engine is a **pure function over injected data** (D4): the caller
//! supplies [`PositivityDefs`] and the SCC to check. It performs no
//! environment lookups, so absence from `defs` is meaningful rather than a
//! lookup failure.
//!
//! # Usage
//!
//! ```ignore
//! let defs = PositivityDefs::new(defs, &aliases, stubs);
//! let occs = param_occurrences(&defs);
//! for group in sccs {
//!     for violation in check_strict_positivity(&group, &defs, &occs) { ... }
//! }
//! ```
//!
//! The group **must** be a true SCC of the complete type graph — ADTs *and*
//! records, with aliases inlined. [`referenced_names`] is the matching edge
//! collector, and it deliberately traverses a *superset* of what the walker
//! traverses: an edge the graph misses splits an SCC, and a split SCC is a
//! false accept.

mod defs;
mod driver;
mod fixpoint;
mod lattice;
mod refs;
mod walker;

use std::collections::{BTreeSet, HashSet};

pub use defs::{FieldRef, PositivityCtor, PositivityDef, PositivityDefs};
pub use driver::{analyze_positivity, PositivityAnalysis};
pub use fixpoint::{param_occurrences, ParamOccs};
pub use lattice::{Mode, Occ};
pub use refs::{head_census, referenced_names, HeadCensus};
pub use walker::ViaLink;

use walker::{Observer, Walk};

/// One non-strictly-positive occurrence, attributed to the constructor field it
/// was found in.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PositivityViolation {
    /// The definition whose field carries the occurrence.
    pub type_name: String,
    /// The constructor; for a record, the record type's own name (D4).
    pub ctor_name: String,
    /// Whether the definition is a record, for diagnostic rendering.
    pub is_record: bool,
    /// Which field of that constructor.
    pub field: FieldRef,
    /// The group member reached at a forbidden position.
    pub occurrence: String,
    /// Intermediate `(type, parameter)` pairs the violation was inherited
    /// through; empty for a direct arrow-domain occurrence.
    pub via: Vec<ViaLink>,
}

/// Check every member of `group` for occurrences reached at
/// [`Mode::Forbidden`].
///
/// `group` is one SCC of the complete type graph — **including singletons**.
/// `type Bad = Mk(Bad -> Bad)` is a size-1 SCC, so a worklist built from the
/// elaborator's `mutual_recursion_groups` (which stores only SCCs of size > 1)
/// would check nothing in exactly this ADR's headline case.
///
/// Violations are deduplicated by identity and returned in a deterministic
/// order (group members sorted, then constructor and field order).
#[must_use]
pub fn check_strict_positivity(
    group: &BTreeSet<String>,
    defs: &PositivityDefs,
    occs: &ParamOccs,
) -> Vec<PositivityViolation> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();

    for member in group {
        let Some(def) = defs.get(member) else {
            continue;
        };
        for ctor in &def.ctors {
            for (field, ty) in &ctor.fields {
                let mut sink = ViolationSink {
                    group,
                    type_name: member,
                    ctor_name: &ctor.name,
                    is_record: def.is_record,
                    field,
                    found: &mut found,
                    seen: &mut seen,
                };
                Walk::new(defs, occs, &def.params, &mut sink).field(ty);
            }
        }
    }
    found
}

/// Records a violation for every group member reached at `Forbidden`.
struct ViolationSink<'a> {
    group: &'a BTreeSet<String>,
    type_name: &'a str,
    ctor_name: &'a str,
    is_record: bool,
    field: &'a FieldRef,
    found: &'a mut Vec<PositivityViolation>,
    seen: &'a mut HashSet<PositivityViolation>,
}

impl Observer for ViolationSink<'_> {
    fn named(&mut self, name: &str, mode: Mode, via: &[ViaLink]) {
        if mode != Mode::Forbidden || !self.group.contains(name) {
            return;
        }
        let violation = PositivityViolation {
            type_name: self.type_name.to_string(),
            ctor_name: self.ctor_name.to_string(),
            is_record: self.is_record,
            field: self.field.clone(),
            occurrence: name.to_string(),
            via: via.to_vec(),
        };
        if self.seen.insert(violation.clone()) {
            self.found.push(violation);
        }
    }
}

#[cfg(test)]
mod tests;
