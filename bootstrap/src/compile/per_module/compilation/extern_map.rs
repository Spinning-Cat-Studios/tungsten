//! Extern-name-map population: the single authority for which LLVM symbol a
//! bare referenced name resolves to (split from `compilation.rs` for the
//! file-size convention; ADR 12.7.26b).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::DefInfo;
use crate::compile::def_llvm_name;

/// Record `original → scoped` extern-name-map entries for the cross-unit defs
/// this unit references — the map-population half of `declare_referenced_externs`,
/// with the LLVM `declare` emission left to the caller.
///
/// Iteration follows `all_defs_info`'s key order, so for a colliding name the
/// surviving entry is the last same-named def in key sort order
/// ("clobber-last", ADR 12.7.26a §1). `doctor check extern-map-ambiguity`
/// calls this same function to report that resolution without an LLVM context
/// (ADR 12.7.26b D2).
pub(in crate::compile) fn extend_extern_map_for_referenced(
    referenced: &BTreeSet<String>,
    all_defs_info: &BTreeMap<String, DefInfo>,
    unit_name: &str,
    extern_name_map: &mut HashMap<String, String>,
) {
    for (key, info) in all_defs_info {
        if info.owner_unit == unit_name {
            continue;
        }
        let original = key.split("::").last().unwrap_or(key);
        if !referenced.contains(original) {
            continue;
        }
        let original_llvm = def_llvm_name(original);
        if info.llvm_name != original_llvm {
            extern_name_map.insert(original_llvm, info.llvm_name.clone());
        }
    }
}
