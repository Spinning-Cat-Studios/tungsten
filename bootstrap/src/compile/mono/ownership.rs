//! Owner assignment: map each `MonoKey` to the `__mono` depot unit.
//!
//! All monomorphized specializations are routed to a single depot file
//! (`__mono.ll`) to keep per-function codegen units clean (ADR 9.5.26b §2.3).

use std::collections::HashMap;

use super::symbols::mangle_mono_symbol;
use super::{CodegenUnitId, MonoKey, MonoOwnership, MonoOwnershipMap, MonoRequestTable};

/// Assign each unique `MonoKey` to the mono depot codegen unit.
///
/// The request table must be frozen before calling this.
///
/// All specializations are routed to `__mono` (ADR 9.5.26b §2.3).
/// The `known_units` parameter is retained for API compatibility but
/// is not used for ownership decisions.
///
/// # Panics
/// Panics if the table is not frozen.
pub fn assign_owners(table: &MonoRequestTable, _known_units: &[String]) -> MonoOwnershipMap {
    assert!(table.is_frozen(), "ICE: ownership assignment before freeze");

    let mut entries = HashMap::new();

    // Build a map from MonoKey → Vec<Type> from the first request for each key
    let mut key_type_args: HashMap<MonoKey, Vec<tungsten_core::types::Type>> = HashMap::new();
    for req in table.requests() {
        key_type_args
            .entry(req.key.clone())
            .or_insert_with(|| req.type_args.clone());
    }

    for key in table.unique_keys() {
        let owner = CodegenUnitId::mono_depot();

        let symbol = mangle_mono_symbol(&key);
        let type_args = key_type_args
            .get(&key)
            .cloned()
            .expect("ICE: unique key without corresponding request");

        entries.insert(
            key.clone(),
            MonoOwnership {
                key,
                owner_unit: owner,
                symbol,
                type_args,
            },
        );
    }

    MonoOwnershipMap::new(entries)
}
