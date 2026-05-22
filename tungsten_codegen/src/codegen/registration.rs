//! Registration methods for `CodeGen`.
//!
//! Handles registering extern name maps, record types, ADT types,
//! term definitions, and definition types into the code generator.

use std::collections::HashMap;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

use super::CodeGen;

impl CodeGen<'_> {
    /// Register extern name mappings for Global lookups.
    pub fn register_extern_name_map(&mut self, map: HashMap<String, String>) {
        self.defs.extern_name_map = map;
    }

    /// Register record types for expansion during type lowering.
    pub fn register_record_types(&mut self, records: HashMap<String, Vec<(String, Type)>>) {
        self.types.register_record_types(records);
    }

    /// Register ADT types for expansion during type lowering.
    pub fn register_adt_types(
        &mut self,
        adts: HashMap<String, (Vec<String>, Vec<crate::types::CodegenConstructor>)>,
    ) {
        self.types.register_adt_types(adts);
    }

    /// Enable codegen backtrace capture on `TyVar` fallthrough (ADR 13.4.26b W3.1 Tool 4).
    pub fn set_codegen_backtrace(&mut self, enabled: bool) {
        self.types.set_codegen_backtrace(enabled);
    }

    /// Enable ADT operation tracing at runtime (T3, ADR 16.4.26a).
    ///
    /// `filter` controls which ADT types are traced:
    /// - `"all"` — trace all ADT construct/match operations
    /// - any other string — only trace operations on ADTs whose name contains the filter
    ///
    /// Declares the runtime tracing functions (`__tungsten_trace_adt_construct`,
    /// `__tungsten_trace_adt_match`) if not already present.
    pub fn set_trace_adt_ops(&mut self, filter: String) {
        self.tracing.trace_adt_ops = Some(filter);

        let i8_ptr = self.context.ptr_type(inkwell::AddressSpace::default());
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let void_type = self.context.void_type();

        // __tungsten_trace_adt_construct(type_name: *const u8, variant_idx: i32,
        //                                data_ptr: *const u8, data_size: u64)
        let construct_ty = void_type.fn_type(
            &[
                i8_ptr.into(),
                i32_type.into(),
                i8_ptr.into(),
                i64_type.into(),
            ],
            false,
        );
        if self
            .module
            .get_function("__tungsten_trace_adt_construct")
            .is_none()
        {
            self.module
                .add_function("__tungsten_trace_adt_construct", construct_ty, None);
        }

        // __tungsten_trace_adt_match(type_name: *const u8, tag: i32,
        //                            data_ptr: *const u8, data_size: u64)
        let match_ty = void_type.fn_type(
            &[
                i8_ptr.into(),
                i32_type.into(),
                i8_ptr.into(),
                i64_type.into(),
            ],
            false,
        );
        if self
            .module
            .get_function("__tungsten_trace_adt_match")
            .is_none()
        {
            self.module
                .add_function("__tungsten_trace_adt_match", match_ty, None);
        }
    }

    /// Enable musttail decision tracing (ADR 8.5.26c).
    ///
    /// When enabled, codegen emits `[musttail] fn_name: EMIT/SKIP (reason)`
    /// to stderr for each self-recursive direct call site.
    pub fn set_trace_musttail(&mut self) {
        self.tracing.trace_musttail = true;
    }

    /// Enable escape analysis tracing (ADR 8.5.26d).
    ///
    /// When enabled, codegen emits `[escape] var: STACK/HEAP (reason)`
    /// to stderr for each fold allocation decision.
    pub fn set_trace_escape(&mut self) {
        self.tracing.trace_escape = true;
    }

    /// Enable allocation profiling (ADR 7.5.26b).
    ///
    /// Declares the profiler runtime functions and switches malloc calls
    /// to use the profiling wrapper. If `filter` is `Some(name)`, only
    /// that function will appear in the report.
    pub fn set_alloc_profile(&mut self, filter: Option<String>) {
        self.tracing.alloc_profile = true;
        self.tracing.alloc_profile_filter = filter;

        let i8_ptr = self.context.ptr_type(inkwell::AddressSpace::default());
        let void_type = self.context.void_type();

        // __tungsten_alloc_profile_set_fn(name: *const c_char) -> void
        let set_fn_ty = void_type.fn_type(&[i8_ptr.into()], false);
        if self
            .module
            .get_function("__tungsten_alloc_profile_set_fn")
            .is_none()
        {
            self.module
                .add_function("__tungsten_alloc_profile_set_fn", set_fn_ty, None);
        }

        // __tungsten_alloc_profile_set_filter(name: *const c_char) -> void
        let set_filter_ty = void_type.fn_type(&[i8_ptr.into()], false);
        if self
            .module
            .get_function("__tungsten_alloc_profile_set_filter")
            .is_none()
        {
            self.module
                .add_function("__tungsten_alloc_profile_set_filter", set_filter_ty, None);
        }

        // __tungsten_alloc_profile_report() -> void
        let report_ty = void_type.fn_type(&[], false);
        if self
            .module
            .get_function("__tungsten_alloc_profile_report")
            .is_none()
        {
            self.module
                .add_function("__tungsten_alloc_profile_report", report_ty, None);
        }

        // __tungsten_alloc_profile_malloc(size: i64) -> *mut void
        let malloc_ty = i8_ptr.fn_type(&[self.context.i64_type().into()], false);
        if self
            .module
            .get_function("__tungsten_alloc_profile_malloc")
            .is_none()
        {
            self.module
                .add_function("__tungsten_alloc_profile_malloc", malloc_ty, None);
        }
    }

    /// Register a term definition for potential monomorphization.
    pub fn register_term_def(&mut self, name: &str, term: Term) {
        self.defs.term_defs.insert(name.to_string(), term);
    }

    /// Bulk-register pre-built poly term definitions (ADR 10.5.26h §2.1).
    /// Replaces per-worker iteration with a single clone of the shared registry.
    pub fn register_term_defs_bulk(&mut self, shared_registry: &HashMap<String, Term>) {
        self.defs.term_defs.clone_from(shared_registry);
    }

    /// Check if a term definition is registered (for testing).
    pub fn has_term_def(&self, name: &str) -> bool {
        self.defs.term_defs.contains_key(name)
    }

    /// Register a definition's type without creating an LLVM declaration.
    /// Used for polymorphic definitions that are compiled on-demand via monomorphization.
    pub fn register_def_type(&mut self, name: &str, ty: &Type) {
        self.defs.def_types.insert(name.to_string(), ty.clone());
    }

    /// Look up a registered definition type by name.
    pub fn get_def_type(&self, name: &str) -> Option<Type> {
        self.defs.def_types.get(name).cloned()
    }

    /// Pre-register a monomorphized instance symbol (ADR 8.5.26g §2.4).
    ///
    /// Seeds `MonomorphState.instances` so that `compile_monomorphized` finds
    /// the pre-assigned symbol instead of generating a fresh per-unit name.
    /// `global_name` is the original polymorphic definition name.
    /// `type_args` is the concrete type argument(s).
    /// `symbol` is the mangled symbol from the mono ownership map.
    pub fn register_mono_instance(&mut self, global_name: &str, type_args: &[Type], symbol: &str) {
        let ty_key = if type_args.len() == 1 {
            format!("{:?}", type_args[0])
        } else {
            let parts: Vec<String> = type_args.iter().map(|t| format!("{t:?}")).collect();
            parts.join(", ")
        };
        let mono_key = (global_name.to_string(), ty_key);
        self.monomorph
            .instances
            .insert(mono_key, symbol.to_string());
    }

    /// Activate the single-owner mono pipeline guard (ADR 8.5.26g §2.4).
    ///
    /// After calling this, `compile_monomorphized` will refuse to generate
    /// fresh per-unit mono instances and instead return an error if an
    /// instance is not found in the pre-seeded map. Call this after all
    /// `register_mono_instance` calls are complete.
    pub fn activate_mono_map(&mut self) {
        self.monomorph.mono_map_active = true;
    }
}
