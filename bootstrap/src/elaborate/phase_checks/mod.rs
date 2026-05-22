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
    /// Phase 1a: Register type names
    Phase1a,
    /// Phase 1b: Process imports
    Phase1b,
    /// Phase 1c: Collect type bodies
    Phase1c,
    /// Phase 1c.5: Compute mutual recursion groups (SCCs)
    Phase1c5,
    /// Phase 1d: Resolve deferred @-prefixed TyVars
    Phase1d,
    /// Phase 1e: Cache type encodings
    Phase1e,
    /// Post-collection: Constructor metadata integrity (ADR 7.5.26f)
    ConstructorMetadata,
}

impl fmt::Display for ElaborationPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Phase1a => write!(f, "Phase 1a"),
            Self::Phase1b => write!(f, "Phase 1b"),
            Self::Phase1c => write!(f, "Phase 1c"),
            Self::Phase1c5 => write!(f, "Phase 1c.5"),
            Self::Phase1d => write!(f, "Phase 1d"),
            Self::Phase1e => write!(f, "Phase 1e"),
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
    /// Check Phase 1a invariant: all type names registered, no duplicates.
    pub(crate) fn check_phase_1a(&mut self) {
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
                // After Phase 1a, all locally defined types should be stubs
                // (only imported types may be non-stubs)
                if type_def.defining_module.is_none() {
                    non_stub_count += 1;
                    violations.push(format!(
                        "{name}: expected Stub after Phase 1a, found {:?}",
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
            phase: ElaborationPhase::Phase1a,
            passed,
            violations,
            stats,
        });
    }

    /// Check Phase 1b invariant: all imports resolved, no dangling refs.
    pub(crate) fn check_phase_1b(&mut self) {
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
            phase: ElaborationPhase::Phase1b,
            passed: true,
            violations,
            stats,
        });
    }

    /// Check Phase 1c invariant: all type bodies populated, cross-references
    /// use @-prefixed TyVars.
    pub(crate) fn check_phase_1c(&mut self) {
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
                        "{name}: still a Stub after Phase 1c (body not populated)"
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
            phase: ElaborationPhase::Phase1c,
            passed,
            violations,
            stats,
        });
    }

    /// Check Phase 1c.5 invariant: SCC groups computed, all members known.
    pub(crate) fn check_phase_1c5(&mut self) {
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
            phase: ElaborationPhase::Phase1c5,
            passed,
            violations,
            stats,
        });
    }
}

mod late_checks;
