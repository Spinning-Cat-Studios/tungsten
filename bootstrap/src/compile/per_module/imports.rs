//! Colliding-reference resolution for per-function codegen units (ADR 12.7.26a).
//!
//! The per-unit extern name map is keyed by bare name and built clobber-last,
//! so a reference to a name defined in ≥ 2 modules silently resolved to
//! whichever def sorted last (ADR 12.7.26a §1 — a silent miscompile). This
//! module is the replacement authority. Resolution order per referenced
//! colliding name:
//!
//! 1. a def in the referencing unit's **own module** (`DefInfo.module_path`);
//! 2. a def in the module the unit's source **imported the name from**
//!    (the per-module import-target table elaboration exports, §2.1);
//! 3. neither, or an ambiguous import → **hard compile error** (D1).
//!
//! Shared by per-function units (`declare_unit_defs`), the `__mono` depot
//! (`compile_mono_depot_unit`, D5), and `doctor check extern-map-ambiguity`
//! (ADR 12.7.26b D2 — the check must never disagree with real codegen).
//!
//! Pure functions over maps — LLVM-free and unit-testable.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use tungsten_bootstrap::driver::ValueImportTargetsByModule;
use tungsten_bootstrap::elaborate::ImportTarget;

use super::compilation::DefInfo;
use crate::compile::def_llvm_name;

/// Borrowed lookup tables for colliding-reference resolution.
pub(in crate::compile) struct CollisionContext<'a> {
    /// Bare LLVM name → same-named `all_defs_info` keys (sorted).
    pub(in crate::compile) collision_index: &'a BTreeMap<String, Vec<String>>,
    pub(in crate::compile) all_defs_info: &'a BTreeMap<String, DefInfo>,
    /// Canonical module path → that module's value import targets (§2.1).
    pub(in crate::compile) import_targets: &'a ValueImportTargetsByModule,
}

/// Outcome of resolving one referenced name against the collision set.
pub(in crate::compile) enum CollisionResolution {
    /// The name is unique across units — the plain extern map entry is correct.
    NotColliding,
    /// Resolved to a specific def's LLVM symbol via unambiguous provenance.
    Resolved(String),
    /// ≥ 2 same-named defs and no unambiguous provenance — a D1 hard error.
    /// Carries the `all_defs_info` keys of every candidate def.
    Unresolvable { candidates: Vec<String> },
}

/// Resolve one referenced name against the collision set (ADR 12.7.26a).
///
/// `own_module_path` is the canonical module path of the referencing unit
/// (or mono instance): a def there wins first; otherwise the module's import
/// table selects the target module; otherwise the reference is unresolvable.
pub(in crate::compile) fn resolve_collision(
    name: &str,
    own_module_path: &[String],
    ctx: &CollisionContext<'_>,
) -> CollisionResolution {
    let original_llvm = def_llvm_name(name);
    let Some(candidates) = ctx.collision_index.get(&original_llvm) else {
        return CollisionResolution::NotColliding;
    };
    if candidates.len() < 2 {
        // A lone candidate can't be ambiguous — the plain map entry is right.
        return CollisionResolution::NotColliding;
    }

    // 1. Own-module def: an unqualified reference to a sibling definition.
    if let Some(symbol) = candidate_in_module(candidates, own_module_path, ctx.all_defs_info) {
        return CollisionResolution::Resolved(symbol);
    }

    // 2. Import-table target: the module the unit's source imported it from.
    if let Some(target) = ctx
        .import_targets
        .get(own_module_path)
        .and_then(|table| table.get(name))
    {
        match target {
            ImportTarget::Unambiguous(target_module) => {
                if let Some(symbol) =
                    candidate_in_module(candidates, target_module, ctx.all_defs_info)
                {
                    return CollisionResolution::Resolved(symbol);
                }
                // The import table names a module with no such def — fall
                // through to the hard error rather than guess.
            }
            ImportTarget::Ambiguous(_) => {
                // Double-alias imports of the same original name (D3): the
                // occurrence can't tell which alias was meant.
            }
        }
    }

    // 3. No unambiguous provenance → hard error (D1).
    CollisionResolution::Unresolvable {
        candidates: candidates.clone(),
    }
}

/// The candidate def (if any) whose owning module is `module_path`.
fn candidate_in_module(
    candidates: &[String],
    module_path: &[String],
    all_defs_info: &BTreeMap<String, DefInfo>,
) -> Option<String> {
    candidates.iter().find_map(|key| {
        let info = all_defs_info.get(key)?;
        (info.module_path == module_path).then(|| info.llvm_name.clone())
    })
}

/// Resolve every referenced colliding name for one unit (or mono instance),
/// or reject with the D1 hard error. On success returns
/// `original_llvm_name → resolved symbol` overrides for the extern name map.
pub(in crate::compile) fn collision_overrides_for_unit(
    own_module_path: &[String],
    unit_label: &str,
    referenced: &BTreeSet<String>,
    ctx: &CollisionContext<'_>,
) -> Result<HashMap<String, String>, String> {
    let mut overrides = HashMap::new();
    for name in referenced {
        match resolve_collision(name, own_module_path, ctx) {
            CollisionResolution::NotColliding => {}
            CollisionResolution::Resolved(symbol) => {
                overrides.insert(def_llvm_name(name), symbol);
            }
            CollisionResolution::Unresolvable { candidates } => {
                return Err(ambiguous_reference_error(
                    name,
                    unit_label,
                    &candidates,
                    ctx.all_defs_info,
                ));
            }
        }
    }
    Ok(overrides)
}

/// The ADR 12.7.26a §2.3 diagnostic. Unit-scoped, not span-scoped:
/// `Term::Global` carries no source span (D3), so the message names the
/// offending unit and the colliding modules instead of a source location.
fn ambiguous_reference_error(
    name: &str,
    unit_label: &str,
    candidates: &[String],
    all_defs_info: &BTreeMap<String, DefInfo>,
) -> String {
    let modules: Vec<String> = candidates
        .iter()
        .map(|key| candidate_module_display(key, all_defs_info))
        .collect();
    let module_list = modules.join("` and `");
    let first = modules.first().cloned().unwrap_or_default();
    format!(
        "ambiguous reference to `{name}` in unit `{unit_label}`: \
         `{name}` is defined in modules `{module_list}`, and this unit does not \
         import it unambiguously; \
         help: add `use {first}::{{{name}}}` (or another candidate module) to \
         select one, or remove one of the conflicting `use ... as ...` imports"
    )
}

/// Human-readable module name for a candidate def key.
///
/// Falls back to the candidate's owner unit for defs without a module path
/// (e.g. synthesized units).
fn candidate_module_display(key: &str, all_defs_info: &BTreeMap<String, DefInfo>) -> String {
    match all_defs_info.get(key) {
        Some(info) if !info.module_path.is_empty() => info.module_path.join("::"),
        Some(info) => info.owner_unit.clone(),
        None => key.to_string(),
    }
}
