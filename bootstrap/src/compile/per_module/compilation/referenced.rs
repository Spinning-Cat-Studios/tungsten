//! Referenced-global collection for codegen units (split from
//! `compilation/mod.rs` for the file-size convention; ADR 12.7.26a).

use std::collections::BTreeSet;

use tungsten_bootstrap::driver::ModuleCodegenUnit;
use tungsten_core::terms::Term;

/// References made by the defs a unit actually compiles: Forall-typed
/// (generic) bodies never lower in their own unit (`compile_unit_defs` skips
/// them), so their references only resolve inside `__mono` depot instances.
/// Shared by `declare_unit_defs` (ADR 12.7.26a) and
/// `check_extern_map_ambiguity` (ADR 12.7.26b).
pub(in crate::compile) fn nongeneric_referenced_globals(
    unit: &ModuleCodegenUnit,
) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    for def in &unit.defs {
        if matches!(&def.ty, tungsten_core::types::Type::Forall(_, _)) {
            continue;
        }
        collect_globals_from_term(&def.term.term, &mut refs);
    }
    refs
}

/// Collect all `Global(name)` references from a codegen unit's definitions.
///
/// Used to filter cross-module declarations: only declare functions that
/// are actually referenced by this unit's term bodies (ADR 9.5.26d §2.2b).
pub(in crate::compile) fn collect_referenced_globals(unit: &ModuleCodegenUnit) -> BTreeSet<String> {
    let mut globals = BTreeSet::new();
    for def in &unit.defs {
        collect_globals_from_term(&def.term.term, &mut globals);
    }
    globals
}

pub(in crate::compile) fn collect_globals_from_term(term: &Term, globals: &mut BTreeSet<String>) {
    if let Term::Global(name) = term {
        globals.insert(name.clone());
    }
    term.for_each_subterm(|child| collect_globals_from_term(child, globals));
}
