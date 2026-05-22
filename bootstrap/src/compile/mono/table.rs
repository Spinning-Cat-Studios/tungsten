//! `MonoRequestTable`: mutable collection of monomorphization requests.
//!
//! Frozen before owner assignment; after freeze, no more requests can be added.

use super::{CodegenUnitId, MonoKey, MonoRequest};

/// Mutable table of monomorphization requests. Frozen before owner assignment.
pub struct MonoRequestTable {
    requests: Vec<MonoRequest>,
    frozen: bool,
}

impl MonoRequestTable {
    pub fn new() -> Self {
        Self {
            requests: Vec::new(),
            frozen: false,
        }
    }

    /// Add a request. Panics if the table is frozen.
    pub fn add(&mut self, request: MonoRequest) {
        assert!(!self.frozen, "ICE: mono request added after freeze point");
        self.requests.push(request);
    }

    /// Freeze the table. No more requests can be added after this.
    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// Deduplicated keys: unique `MonoKey`s across all requests.
    pub fn unique_keys(&self) -> Vec<MonoKey> {
        let mut seen = std::collections::HashSet::new();
        let mut keys = Vec::new();
        for req in &self.requests {
            if seen.insert(req.key.clone()) {
                keys.push(req.key.clone());
            }
        }
        keys.sort();
        keys
    }

    pub fn requests(&self) -> &[MonoRequest] {
        &self.requests
    }

    /// Unique keys requested by a specific codegen unit.
    pub fn keys_requested_by(&self, unit: &CodegenUnitId) -> Vec<MonoKey> {
        let mut seen = std::collections::HashSet::new();
        let mut keys = Vec::new();
        for req in &self.requests {
            if &req.requester_unit == unit && seen.insert(req.key.clone()) {
                keys.push(req.key.clone());
            }
        }
        keys
    }
}
