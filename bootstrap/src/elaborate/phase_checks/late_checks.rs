//! Late-phase elaboration checks (1d, 1e, constructor metadata).

use super::tyvar_collectors::collect_at_prefixed_tyvars;
use super::{ElaborationPhase, PhaseCheckResult};
use crate::elaborate::env::TypeDefKind;
use crate::elaborate::Elaborator;
use tungsten_core::Type;

impl<'a> Elaborator<'a> {
    /// Check Phase 1d invariant: no unresolved @-TyVars referencing unknown types.
    ///
    /// After Phase 1d, @-TyVars legitimately remain for:
    /// - ADTs in mutual recursion groups (group members stay as TyVars for μ-encoding)
    /// - Types with circular dependencies (mu_encoding_stack prevents infinite expansion)
    ///
    /// This check flags @-TyVars that refer to undefined types (genuine errors).
    /// Cross-references to defined types in circular dependencies are expected.
    pub(crate) fn check_phase_1d(&mut self) {
        if !self.check_phase_invariants {
            return;
        }

        let mut violations = Vec::new();
        let mut total_checked = 0;
        let mut deferred_circular = 0;

        let type_entries: Vec<(String, _)> = self
            .env
            .iter_types()
            .filter(|(_, td)| td.defining_module.is_none())
            .map(|(name, td)| (name.clone(), td.kind.clone()))
            .collect();

        for (name, kind) in &type_entries {
            let mut at_vars = Vec::new();
            match kind {
                TypeDefKind::ADT(ctors) => {
                    for ctor in ctors {
                        for field in &ctor.fields {
                            collect_at_prefixed_tyvars(field, &mut at_vars);
                        }
                    }
                }
                TypeDefKind::Record(fields) => {
                    for (_, ty) in fields {
                        collect_at_prefixed_tyvars(ty, &mut at_vars);
                    }
                }
                TypeDefKind::Alias(ty) => {
                    collect_at_prefixed_tyvars(ty, &mut at_vars);
                }
                TypeDefKind::Stub => {}
            }

            total_checked += 1;

            for var in &at_vars {
                let ref_name = var.trim_start_matches('@');
                let is_group_member = self
                    .mutual_recursion_groups
                    .get(name)
                    .map_or(false, |g| g.contains(&ref_name.to_string()));
                let is_known_type = self.env.lookup_type(ref_name).is_some();

                if is_group_member || is_known_type {
                    deferred_circular += 1;
                } else {
                    violations.push(format!(
                        "{name}: unresolved @-prefixed TyVar {var} (type not found)"
                    ));
                }
            }
        }

        let passed = violations.is_empty();
        let stats = format!(
            "{} type(s) checked, {} deferred circular ref(s), {} unresolved",
            total_checked,
            deferred_circular,
            violations.len()
        );

        self.phase_invariant_results.push(PhaseCheckResult {
            phase: ElaborationPhase::Phase1d,
            passed,
            violations,
            stats,
        });
    }

    /// Check Phase 1e invariant: all non-parameterized types have cached
    /// encodings, no unexpected TyVar escapes in encodings.
    pub(crate) fn check_phase_1e(&mut self) {
        if !self.check_phase_invariants {
            return;
        }

        let mut violations = Vec::new();
        let mut cached_count = 0;
        let mut param_count = 0;
        let mut escape_count = 0;

        let type_entries: Vec<(String, Vec<String>, Option<Type>)> = self
            .env
            .iter_types()
            .filter(|(_, td)| td.defining_module.is_none())
            .filter(|(_, td)| !matches!(td.kind, TypeDefKind::Stub))
            .map(|(name, td)| (name.clone(), td.params.clone(), td.encoded_type.clone()))
            .collect();

        for (name, params, encoded) in &type_entries {
            if params.is_empty() {
                if let Some(enc) = encoded {
                    cached_count += 1;

                    let mut at_vars = Vec::new();
                    collect_at_prefixed_tyvars(enc, &mut at_vars);
                    for var in &at_vars {
                        let ref_name = var.trim_start_matches('@');
                        if self.env.lookup_type(ref_name).is_none() {
                            violations.push(format!(
                                "{name}: @-prefixed TyVar escape to unknown type: {var}"
                            ));
                            escape_count += 1;
                        }
                    }
                } else {
                    violations.push(format!(
                        "{name}: non-parameterized type missing cached encoding"
                    ));
                }
            } else {
                param_count += 1;
            }
        }

        let passed = violations.is_empty();
        let stats = format!(
            "{} encoding(s) cached, {} parameterized type(s) skipped, {} escape(s)",
            cached_count, param_count, escape_count
        );

        self.phase_invariant_results.push(PhaseCheckResult {
            phase: ElaborationPhase::Phase1e,
            passed,
            violations,
            stats,
        });
    }

    /// Count distinct mutual recursion groups (dedup by sorted member list).
    pub(crate) fn count_distinct_groups(&self) -> usize {
        let mut seen: Vec<Vec<String>> = Vec::new();
        for group in self.mutual_recursion_groups.values() {
            let mut sorted = group.clone();
            sorted.sort();
            if !seen.contains(&sorted) {
                seen.push(sorted);
            }
        }
        seen.len()
    }

    /// Check constructor metadata integrity (ADR 7.5.26f §2.4).
    pub(crate) fn check_constructor_metadata(&mut self) {
        if !self.check_phase_invariants {
            return;
        }

        let mut violations = Vec::new();
        let mut ok_count = 0;

        let adt_types = self.get_adt_types();
        let mut adt_names: Vec<&String> = adt_types.keys().collect();
        adt_names.sort();

        for name in adt_names {
            let (_, constructors) = &adt_types[name];
            let result = crate::doctor::checks::check_constructor_counts::validate_constructors(
                name,
                constructors,
            );
            if result.is_ok() {
                ok_count += 1;
            } else {
                for v in &result.violations {
                    violations.push(format!("{name}: {v:?}"));
                }
            }
        }

        let total = adt_types.len();
        let passed = violations.is_empty();
        let stats = format!("{ok_count}/{total} ADT(s) pass constructor metadata integrity");

        self.phase_invariant_results.push(PhaseCheckResult {
            phase: ElaborationPhase::ConstructorMetadata,
            passed,
            violations,
            stats,
        });
    }
}
