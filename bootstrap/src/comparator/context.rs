//! What comparator synthesis needs to know about a project's types
//! (ADRs 1.8.26b D2, 1.8.26c).
//!
//! Four maps, each here because something the comparator must resolve is
//! recorded nowhere else: **records** for a named record's fields,
//! **μ-members** for the cluster bodies an encoding does not carry (below),
//! **ADT definitions** for the *generic* bodies no stored encoding has, and
//! **recursion groups** so an expansion binds a cluster sibling to its binder
//! rather than leaving a dangling name.
//!
//! ## Why records alone are not enough
//!
//! A mutually recursive cluster is encoded with **one nested μ-binder per SCC
//! member**, all wrapping the *entry member's* body (`CLAUDE.md` § μ-type
//! encoding; ADR 18.4.26i). For the cluster
//! `Alpha = ANil | ACons(Beta, Alpha)` / `Beta = BNil | BCons(Gamma, Beta)` /
//! `Gamma = GLeaf(Nat) | GNest(Alpha)` the stored encodings are:
//!
//! ```text
//! Alpha = μα_Alpha. μα_Beta.  μα_Gamma. (Unit + (α_Beta  × α_Alpha))
//! Beta  = μα_Beta.  μα_Alpha. μα_Gamma. (Unit + (α_Gamma × α_Beta))
//! Gamma = μα_Gamma. μα_Alpha. μα_Beta.  (Nat  + α_Alpha)
//! ```
//!
//! Read `Alpha` as a closed type and `α_Beta` denotes `μα_Beta. μα_Gamma.
//! (Unit + (α_Beta × α_Alpha))` — i.e. "Beta = Unit + (Beta × Alpha)", which is
//! not what `Beta` is. **The encoding does not carry the other members'
//! bodies.** `α_Beta` is a *marker* saying "Beta goes here", and only the
//! elaborator's provenance table knows what Beta is.
//!
//! That is why no purely local rewrite of the comparator can fix mutual
//! recursion: neither a canonicalising mangle nor a forward declaration can
//! recover a body the type never wrote down. Synthesis needs the map, so
//! synthesis is given the map.

use std::collections::HashMap;

use tungsten_core::Type;

use crate::driver::{AdtTypes, RecordTypes};
use crate::elaborate::TypeProvenance;

use super::expand::{expand_application, AdtDefinitions};

/// Everything about a project's types that comparator synthesis resolves
/// against, in one value.
///
/// Bundled rather than passed as loose maps because the walk threads it through
/// every recursive site: `check_comparable_rec`, `is_supported_rec` and
/// `comparator_body` all need the same view, and a call site that supplied only
/// some of it would make the checker and the synthesiser disagree.
#[derive(Debug, Clone, Default)]
pub struct ComparatorTypes {
    /// Named record types → their fields, for `record_comparator`.
    records: RecordTypes,
    /// μ-binder name (`α_Beta`) → the stored encoding of the cluster member it
    /// stands for. Absent for a binder whose member has no standalone stored
    /// encoding — a *generic* ADT such as `List<T>`, where the binder is the
    /// enclosing type itself and needs no lookup.
    mu_members: HashMap<String, Type>,
    /// ADT definitions (`name → (params, constructors)`) — the only map that
    /// carries *generic* bodies, so the only one an instantiation such as
    /// `List<TypeParam>` can be resolved through (ADR 1.8.26c).
    adts: AdtTypes,
    /// Stored encodings, kept whole (not only as `mu_members`) so an expansion
    /// can inline a reference to another ADT the way the encoder does.
    encoded: HashMap<String, Type>,
    /// Mutual-recursion SCC groups, so an expansion binds a sibling reference
    /// to that sibling's μ-binder instead of leaving a dangling name.
    groups: HashMap<String, Vec<String>>,
}

impl ComparatorTypes {
    /// Build from a project's records, stored encodings, μ-provenance, ADT
    /// definitions and recursion groups.
    ///
    /// A binder maps to a member only when provenance names an ADT that has a
    /// stored encoding. A generic ADT (`List<T>`) has no monomorphic stored
    /// encoding, so its binder is deliberately absent: it is the enclosing
    /// type, which synthesis substitutes directly.
    #[must_use]
    pub fn new(
        records: RecordTypes,
        encoded_types: &HashMap<String, Type>,
        provenance: &TypeProvenance,
        adt_types: AdtTypes,
        mutual_recursion_groups: &HashMap<String, Vec<String>>,
    ) -> Self {
        // Strip the Phase-1c `@`-prefix once, here, rather than at each of the
        // places a stored encoding is spliced into a type the walk will see.
        //
        // It is an elaboration-internal convention that must not leak
        // downstream (`driver::project` strips it from every `CoreDef` for the
        // same reason), and `records()` is keyed without it — so an occurrence
        // that survives is reported as `@Ident is not defined`, an
        // incomplete-closure cause naming a record that is right there. It
        // reached the walk through `mu_member`: `classify` strips the type it
        // is *given*, but a cluster member resolved from provenance is spliced
        // in afterwards. Measured on `src/compiler/test_ast_compare.tg`: 81 of
        // the 81 remaining incomplete closures, `@Ident` and `@CIRCapture`.
        let encoded: HashMap<String, Type> = encoded_types
            .iter()
            .map(|(name, ty)| (name.clone(), ty.strip_tyvar_at_prefix()))
            .collect();
        let mu_members = provenance
            .mu_origins
            .iter()
            .filter_map(|(binder, origin)| {
                encoded
                    .get(&origin.adt_name)
                    .map(|ty| (binder.clone(), ty.clone()))
            })
            .collect();
        ComparatorTypes {
            records,
            mu_members,
            adts: adt_types,
            encoded,
            groups: mutual_recursion_groups.clone(),
        }
    }

    /// Records only, for callers that have no elaborated project to resolve
    /// against (unit tests, and any path that compares no recursive cluster).
    ///
    /// Resolves **no** instantiations: with an empty `adts` map, `List<Nat>` is
    /// refused by path rather than silently accepted.
    #[must_use]
    pub fn from_records(records: RecordTypes) -> Self {
        ComparatorTypes {
            records,
            ..ComparatorTypes::default()
        }
    }

    /// The named record types.
    #[must_use]
    pub fn records(&self) -> &RecordTypes {
        &self.records
    }

    /// The cluster member a μ-binder stands for, if the project defines one.
    #[must_use]
    pub fn mu_member(&self, binder: &str) -> Option<&Type> {
        self.mu_members.get(binder)
    }

    /// The encoding a named ADT application stands for, or `None` when the
    /// project defines no such ADT.
    ///
    /// The single seam through which both the support predicate and the body
    /// builder resolve an instantiation, so they cannot disagree about what
    /// `List<TypeParam>` means (ADR 1.8.26c).
    #[must_use]
    pub fn expand_adt(&self, name: &str, args: &[Type]) -> Option<Type> {
        expand_application(
            name,
            args,
            &AdtDefinitions {
                adts: &self.adts,
                encoded: &self.encoded,
                groups: &self.groups,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elaborate::AdtOrigin;

    fn provenance(entries: &[(&str, &str)]) -> TypeProvenance {
        let mut p = TypeProvenance::default();
        for (binder, adt) in entries {
            p.mu_origins.insert(
                (*binder).to_string(),
                AdtOrigin {
                    adt_name: (*adt).to_string(),
                    type_args: vec![],
                    constructors: vec![],
                },
            );
        }
        p
    }

    /// A project with no ADTs and no recursion groups — the two inputs added at
    /// ADR 1.8.26c, which these μ-member assertions do not exercise.
    fn from_encodings(
        records: RecordTypes,
        encoded: &HashMap<String, Type>,
        provenance: &TypeProvenance,
    ) -> ComparatorTypes {
        ComparatorTypes::new(
            records,
            encoded,
            provenance,
            AdtTypes::new(),
            &HashMap::new(),
        )
    }

    #[test]
    fn a_binder_resolves_to_its_members_stored_encoding() {
        let mut encoded = HashMap::new();
        encoded.insert("Beta".to_string(), Type::Nat);
        let types = from_encodings(
            RecordTypes::new(),
            &encoded,
            &provenance(&[("α_Beta", "Beta")]),
        );
        assert_eq!(types.mu_member("α_Beta"), Some(&Type::Nat));
    }

    #[test]
    fn a_binder_whose_member_has_no_stored_encoding_is_absent() {
        // The generic-ADT case: `List<T>` has no monomorphic stored encoding,
        // and its binder must NOT resolve to something else — an entry here
        // would silently redirect `List`'s own recursion. Since ADR 1.8.26c
        // that binder needs no entry at all: an instantiation is resolved
        // through `expand_adt`, whose outermost binder substitutes to the
        // operand.
        let types = from_encodings(
            RecordTypes::new(),
            &HashMap::new(),
            &provenance(&[("α_List", "List")]),
        );
        assert_eq!(types.mu_member("α_List"), None);
    }

    #[test]
    fn from_records_resolves_no_members_and_no_instantiations() {
        let types = ComparatorTypes::from_records(RecordTypes::new());
        assert_eq!(types.mu_member("α_Beta"), None);
        assert!(types.records().is_empty());
        // The promise `from_records` makes to its callers: an instantiation is
        // *unresolvable*, so it is refused rather than silently accepted.
        assert_eq!(types.expand_adt("List", &[Type::Nat]), None);
    }

    /// A μ-member is spliced into a type the walk then inspects, so it must
    /// arrive `@`-free: `records()` is keyed without the prefix, and an
    /// occurrence that survives reports `@Ident is not defined` about a record
    /// the project defines (ADR 1.8.26c).
    #[test]
    fn a_resolved_mu_member_carries_no_at_prefix() {
        let mut encoded = HashMap::new();
        encoded.insert(
            "Beta".to_string(),
            Type::product(Type::TyVar("@Ident".into()), Type::Nat),
        );
        let types = from_encodings(
            RecordTypes::new(),
            &encoded,
            &provenance(&[("α_Beta", "Beta")]),
        );
        assert_eq!(
            types.mu_member("α_Beta"),
            Some(&Type::product(Type::TyVar("Ident".into()), Type::Nat))
        );
    }

    #[test]
    fn records_survive_construction_from_a_project() {
        let mut records = RecordTypes::new();
        records.insert("Span".to_string(), vec![("start".to_string(), Type::Nat)]);
        let types = from_encodings(records, &HashMap::new(), &TypeProvenance::default());
        assert!(types.records().contains_key("Span"));
    }
}
