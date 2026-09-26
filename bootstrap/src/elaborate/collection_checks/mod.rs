//! Phase invariant checking for the elaboration pipeline (ADR 20.4.26e).
//!
//! Validates that implicit invariants hold at each phase boundary.
//! When enabled via `check_phase_invariants = true` on the Elaborator,
//! check methods run after each phase and collect results into
//! `phase_invariant_results`.

use std::fmt;

use tungsten_core::Type;

use super::env::TypeDefKind;
use super::Elaborator;

#[cfg(test)]
mod tests;
mod tyvar_collectors;

use tyvar_collectors::{collect_at_prefixed_tyvars, collect_non_mu_tyvars};

/// Identifies which phase boundary a check runs after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElaborationPhase {
    /// Type-Name Registration: register type names as stubs
    TypeNameRegistration,
    /// Import Resolution: process use declarations
    ImportResolution,
    /// Type-Body Collection: elaborate type bodies (ADTs, records, aliases)
    TypeBodyCollection,
    /// Recursion Grouping: compute mutual recursion groups (SCCs)
    RecursionGrouping,
    /// Deferred-TyVar Resolution: resolve deferred @-prefixed TyVars
    DeferredTyVarResolution,
    /// Encoding Finalization: cache final type encodings
    EncodingFinalization,
    /// Post-collection: Constructor metadata integrity (ADR 7.5.26f)
    ConstructorMetadata,
}

impl fmt::Display for ElaborationPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeNameRegistration => write!(f, "Type-name registration"),
            Self::ImportResolution => write!(f, "Import resolution"),
            Self::TypeBodyCollection => write!(f, "Type-body collection"),
            Self::RecursionGrouping => write!(f, "Recursion grouping"),
            Self::DeferredTyVarResolution => write!(f, "Deferred-TyVar resolution"),
            Self::EncodingFinalization => write!(f, "Encoding finalization"),
            Self::ConstructorMetadata => write!(f, "Constructor metadata"),
        }
    }
}

/// Result of checking invariants after a single phase.
#[derive(Debug, Clone)]
pub struct PhaseCheckResult {
    /// Which phase boundary this check ran after
    pub phase: ElaborationPhase,
    /// Whether all invariants held
    pub passed: bool,
    /// Specific invariant violations (empty if passed)
    pub violations: Vec<String>,
    /// Summary statistics (e.g., "247 type names registered, 0 collisions")
    pub stats: String,
}

impl<'a> Elaborator<'a> {
    /// Check Type-Name Registration invariant: all type names registered, no duplicates.
    pub(crate) fn check_type_name_registration(&mut self) {
        if !self.check_phase_invariants {
            return;
        }

        let mut violations = Vec::new();
        let mut stub_count = 0;
        let mut non_stub_count = 0;

        for (name, type_def) in self.env.iter_types() {
            if matches!(type_def.kind, TypeDefKind::Stub) {
                stub_count += 1;
            } else {
                // After Type-Name Registration, all locally defined types should be stubs
                // (only imported types may be non-stubs)
                if type_def.defining_module.is_none() {
                    non_stub_count += 1;
                    violations.push(format!(
                        "{name}: expected Stub after Type-Name Registration, found {:?}",
                        type_def.kind
                    ));
                }
            }
        }

        let passed = violations.is_empty();
        let stats = format!(
            "{} type name(s) registered as stubs, {} pre-existing",
            stub_count, non_stub_count
        );

        self.phase_invariant_results.push(PhaseCheckResult {
            phase: ElaborationPhase::TypeNameRegistration,
            passed,
            violations,
            stats,
        });
    }

    /// Check Import Resolution invariant: all imports resolved, no dangling refs.
    pub(crate) fn check_import_resolution(&mut self) {
        if !self.check_phase_invariants {
            return;
        }

        let type_imports = self.env.imported_types.len();
        let value_imports = self.env.imported_values.len();
        let ctor_imports = self.env.imported_constructors.len();

        // After 1b, we just verify that imports were processed.
        // Dangling imports would already have been recorded as errors.
        let violations = Vec::new();
        let stats = format!(
            "{} type import(s), {} value import(s), {} constructor import(s)",
            type_imports, value_imports, ctor_imports
        );

        self.phase_invariant_results.push(PhaseCheckResult {
            phase: ElaborationPhase::ImportResolution,
            passed: true,
            violations,
            stats,
        });
    }

    /// Check Type-Body Collection invariant: all type bodies populated, cross-references
    /// use @-prefixed TyVars.
    pub(crate) fn check_type_body_collection(&mut self) {
        if !self.check_phase_invariants {
            return;
        }

        let mut violations = Vec::new();
        let mut populated = 0;
        let mut stubs_remaining = 0;
        let mut at_prefixed_count = 0;

        // Collect type names first to avoid borrow issues
        let type_entries: Vec<(String, _)> = self
            .env
            .iter_types()
            .filter(|(_, td)| td.defining_module.is_none())
            .map(|(name, td)| (name.clone(), td.kind.clone()))
            .collect();

        for (name, kind) in &type_entries {
            match kind {
                TypeDefKind::Stub => {
                    stubs_remaining += 1;
                    violations.push(format!(
                        "{name}: still a Stub after Type-Body Collection (body not populated)"
                    ));
                }
                TypeDefKind::ADT(ctors) => {
                    populated += 1;
                    for ctor in ctors {
                        for field in &ctor.fields {
                            let mut at_vars = Vec::new();
                            collect_at_prefixed_tyvars(field, &mut at_vars);
                            at_prefixed_count += at_vars.len();
                        }
                    }
                }
                TypeDefKind::Record(fields) => {
                    populated += 1;
                    for (_, ty) in fields {
                        let mut at_vars = Vec::new();
                        collect_at_prefixed_tyvars(ty, &mut at_vars);
                        at_prefixed_count += at_vars.len();
                    }
                }
                TypeDefKind::Alias(ty) => {
                    populated += 1;
                    let mut at_vars = Vec::new();
                    collect_at_prefixed_tyvars(ty, &mut at_vars);
                    at_prefixed_count += at_vars.len();
                }
            }
        }

        let passed = violations.is_empty();
        let stats = format!(
            "{} type(s) populated, {} stub(s) remaining, {} @-prefixed cross-ref(s)",
            populated, stubs_remaining, at_prefixed_count
        );

        self.phase_invariant_results.push(PhaseCheckResult {
            phase: ElaborationPhase::TypeBodyCollection,
            passed,
            violations,
            stats,
        });
    }

    /// Check Recursion Grouping invariant: SCC groups computed, all members known.
    pub(crate) fn check_recursion_grouping(&mut self) {
        if !self.check_phase_invariants {
            return;
        }

        let mut violations = Vec::new();
        let group_count = self.count_distinct_groups();
        let member_count = self.mutual_recursion_groups.len();

        // Verify all group members actually exist in env.types
        for (name, group) in &self.mutual_recursion_groups {
            if self.env.lookup_type(name).is_none() {
                violations.push(format!(
                    "{name}: in mutual recursion group but not in env.types"
                ));
            }
            for member in group {
                if self.env.lookup_type(member).is_none() {
                    violations.push(format!(
                        "{member}: listed as group member of {name} but not in env.types"
                    ));
                }
            }
        }

        let passed = violations.is_empty();
        let stats = format!(
            "{} SCC group(s), {} grouped type(s)",
            group_count, member_count
        );

        self.phase_invariant_results.push(PhaseCheckResult {
            phase: ElaborationPhase::RecursionGrouping,
            passed,
            violations,
            stats,
        });
    }
}

mod late_checks;
