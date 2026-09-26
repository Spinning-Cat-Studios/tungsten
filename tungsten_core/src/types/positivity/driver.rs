//! The whole-corpus driver: group the type graph, then check every group
//! (ADR 18.8.26b D3).
//!
//! [`check_strict_positivity`] is the *predicate*, and it takes one SCC at a
//! time. Deciding a whole corpus additionally needs the SCC pass and the
//! per-group fold — and the bootstrap already has one of those, wired to its
//! own `TypeGraph` in `bootstrap/src/elaborate/positivity/`. The parent ADR's
//! D1(b) forbids relocating it, so this module is a **second driver over the
//! same predicate and the same adjacency**: [`referenced_names`] returns
//! exactly [`Adjacency`], so no conversion happens on the way in.
//!
//! Two drivers over one predicate is a real (if small) divergence surface, so
//! `bootstrap/src/elaborate/positivity/tests/mirror_agreement.rs` asserts the
//! two agree on **both** the group set and the violation set. Without that
//! test the "one engine cannot disagree with itself" argument would rest on
//! the predicate alone, which is not where a grouping bug would live.

use std::collections::BTreeSet;

use crate::terms::termination::tarjan_scc;

use super::{
    check_strict_positivity, param_occurrences, referenced_names, PositivityDefs,
    PositivityViolation,
};

/// What one whole-corpus pass computed.
///
/// The groups are carried alongside the violations rather than discarded,
/// because the agreement test needs them: a driver that grouped differently
/// but happened to find the same violations on today's corpus is exactly the
/// drift R1 names.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PositivityAnalysis {
    /// Every SCC of the expanded graph, singletons included, each sorted.
    pub groups: Vec<BTreeSet<String>>,
    /// Violations in group order, then constructor and field order.
    pub violations: Vec<PositivityViolation>,
}

/// Group `defs` into SCCs and check each group for forbidden occurrences.
///
/// **Singletons are checked too.** `type Bad = Mk(Bad -> Bad)` is a size-1 SCC
/// with a self-edge, so a worklist of "mutually recursive groups" would check
/// nothing in the headline case.
#[must_use]
pub fn analyze_positivity(defs: &PositivityDefs) -> PositivityAnalysis {
    // One fixpoint over the whole graph, not one per group: the parameter
    // strictness of `List` does not depend on which SCC is being checked, and
    // recomputing it per group is quadratic for no gain.
    let occs = param_occurrences(defs);

    let groups: Vec<BTreeSet<String>> = tarjan_scc(&referenced_names(defs))
        .into_iter()
        .map(|component| component.into_iter().collect())
        .collect();

    let violations = groups
        .iter()
        .flat_map(|group| check_strict_positivity(group, defs, &occs))
        .collect();

    PositivityAnalysis { groups, violations }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::positivity::tests::{adt, env, tv};
    use crate::types::Type;

    fn group_set(analysis: &PositivityAnalysis) -> Vec<Vec<String>> {
        let mut groups: Vec<Vec<String>> = analysis
            .groups
            .iter()
            .map(|g| g.iter().cloned().collect())
            .collect();
        groups.sort();
        groups
    }

    /// The headline case is a size-1 SCC with a self-edge, so a driver that
    /// only checked groups of two or more would find nothing here.
    #[test]
    fn singleton_self_arrow_is_grouped_and_rejected() {
        let defs = env(vec![adt(
            "Bad",
            &[],
            vec![("Mk", vec![Type::arrow(tv("Bad"), tv("Bad"))])],
        )]);

        let analysis = analyze_positivity(&defs);

        assert_eq!(group_set(&analysis), vec![vec!["Bad".to_string()]]);
        assert_eq!(analysis.violations.len(), 1);
        assert_eq!(analysis.violations[0].type_name, "Bad");
        assert_eq!(analysis.violations[0].occurrence, "Bad");
    }

    /// A mutual pair must come back as ONE group: checked per type, neither
    /// `A` nor `B` reaches itself and both false-accept.
    #[test]
    fn mutual_pair_is_one_group() {
        let defs = env(vec![
            adt(
                "A",
                &[],
                vec![("MkA", vec![Type::arrow(tv("B"), Type::Nat)])],
            ),
            adt("B", &[], vec![("MkB", vec![tv("A")])]),
        ]);

        let analysis = analyze_positivity(&defs);

        assert_eq!(
            group_set(&analysis),
            vec![vec!["A".to_string(), "B".to_string()]]
        );
        assert_eq!(analysis.violations.len(), 1);
    }

    /// Acceptance is not vacuous: a corpus with no forbidden occurrence still
    /// produces groups, so "0 violations" is distinguishable from "0 examined".
    #[test]
    fn accepted_corpus_has_groups_and_no_violations() {
        let defs = env(vec![
            adt(
                "Ok",
                &[],
                vec![("MkOk", vec![Type::arrow(Type::Nat, tv("Ok"))])],
            ),
            adt("Leaf", &[], vec![("MkLeaf", vec![Type::Nat])]),
        ]);

        let analysis = analyze_positivity(&defs);

        assert_eq!(analysis.groups.len(), 2);
        assert!(analysis.violations.is_empty());
    }

    /// An empty corpus produces an empty analysis rather than a panic — the
    /// gate's caller distinguishes the two, so the driver must not conflate
    /// them by failing.
    #[test]
    fn empty_corpus_is_empty_not_a_panic() {
        let analysis = analyze_positivity(&env(vec![]));

        assert_eq!(analysis, PositivityAnalysis::default());
    }
}
