//! Type registration, substitution management, and diagnostic methods for `TypeLowering`.

use std::collections::HashMap;
use tungsten_core::types::Type;

use super::{AdtDef, TypeLowering};

impl TypeLowering<'_> {
    /// Number of times a `TyVar` fell through to the opaque-pointer fallback in `lower_type`.
    /// Non-zero after compilation indicates unresolved type variables reached codegen.
    #[must_use]
    pub fn tyvar_fallthrough_count(&self) -> usize {
        self.tyvar_fallthrough_count
    }

    /// Emit a summary warning if `TyVar` fallthroughs exceeded the per-occurrence cap.
    /// Call after compilation completes.
    pub fn emit_tyvar_fallthrough_summary(&self) {
        if self.tyvar_fallthrough_count > 5 {
            eprintln!(
                "[codegen warning] {} total TyVar fallthroughs (showing first 5)",
                self.tyvar_fallthrough_count
            );
        }
    }

    /// Register record types for expansion during type lowering.
    /// This allows `TyVar("RecordName")` to be lowered to the structural product type.
    pub fn register_record_types(&mut self, records: HashMap<String, Vec<(String, Type)>>) {
        self.record_types = records;
        self.rebuild_concrete_type_names();
    }

    /// Register ADT types for expansion during type lowering.
    /// This allows `Type::App("Name", args)` to be lowered to sum/mu types.
    pub fn register_adt_types(&mut self, adts: HashMap<String, AdtDef>) {
        self.adt_types = adts;
        self.rebuild_concrete_type_names();
    }

    /// Rebuild the cached `concrete_type_names` set from current ADT + record keys.
    fn rebuild_concrete_type_names(&mut self) {
        self.concrete_type_names = self
            .adt_types
            .keys()
            .chain(self.record_types.keys())
            .cloned()
            .collect();
    }

    /// Push a type substitution for monomorphization.
    /// When lowering type variables, this substitution will be applied first.
    pub fn push_type_subst(&mut self, var: String, ty: Type) {
        self.type_subst.insert(var, ty);
    }

    /// Clear all type substitutions.
    pub fn clear_type_subst(&mut self) {
        self.type_subst.clear();
    }

    /// Restore type substitutions from a saved state.
    pub fn restore_type_subst(&mut self, saved: HashMap<String, Type>) {
        self.type_subst = saved;
    }

    /// Get current type substitutions.
    #[must_use]
    pub fn type_subst(&self) -> &HashMap<String, Type> {
        &self.type_subst
    }
}
