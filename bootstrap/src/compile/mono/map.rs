//! `MonoOwnershipMap`: immutable assignment of mono instances to owner units.
//!
//! Constructed after the request table is frozen. Read-only during codegen.

use std::collections::HashMap;

use super::{CodegenUnitId, MonoKey, MonoOwnership};

/// Immutable map from `MonoKey` to its ownership assignment.
///
/// Constructed after the request table is frozen. Read-only during codegen.
pub struct MonoOwnershipMap {
    entries: HashMap<MonoKey, MonoOwnership>,
}

impl MonoOwnershipMap {
    pub fn new(entries: HashMap<MonoKey, MonoOwnership>) -> Self {
        Self { entries }
    }

    /// Look up ownership for a mono key. Returns `None` if not found.
    pub fn get(&self, key: &MonoKey) -> Option<&MonoOwnership> {
        self.entries.get(key)
    }

    /// Look up ownership, panicking with ICE if absent (codegen path).
    #[allow(dead_code)] // Stage 5 (ADR 8.5.26g): used once mono pipeline is wired into codegen
    pub fn get_or_ice(&self, key: &MonoKey) -> &MonoOwnership {
        self.entries.get(key).unwrap_or_else(|| {
            panic!(
                "ICE: monomorphized call absent from frozen ownership map: {}",
                key
            )
        })
    }

    /// All entries owned by a specific unit, sorted by symbol for deterministic emission order.
    pub fn owned_by(&self, unit: &CodegenUnitId) -> Vec<&MonoOwnership> {
        let mut owned: Vec<_> = self
            .entries
            .values()
            .filter(|o| &o.owner_unit == unit)
            .collect();
        owned.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        owned
    }

    /// All entries (for validation).
    pub fn entries(&self) -> &HashMap<MonoKey, MonoOwnership> {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)] // Stage 5 (ADR 8.5.26g): used once mono pipeline is wired into codegen
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
