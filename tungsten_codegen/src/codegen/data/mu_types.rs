//! Shared helpers for μ-type (recursive type) handling.
//!
//! μ-types are represented as opaque pointers at the LLVM level.
//! The underlying data is heap-allocated via malloc (or stack-allocated
//! via alloca when escape analysis proves the value is non-escaping).
//!
//! ADT construction uses a two-step allocation:
//! 1. `AdtConstruct` → `alloca` (builds the struct payload on the stack)
//! 2. `Fold` → `malloc` (wraps the μ-type; copies the struct to the heap)
//!
//! When escape analysis marks a fold as non-escaping, step 2 uses `alloca`
//! instead of `malloc` (see `fold_to_stack`).
//!
//! - `fold` allocates the inner struct on the heap and returns a pointer
//! - `unfold` dereferences the pointer to get the inner struct
//!
//! This module provides shared utilities used by both sums.rs and adt.rs
//! to avoid code duplication.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValue, BasicValueEnum, PointerValue};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tungsten_core::types::Type;

/// ADR 7.7.26k sizing experiment: per-μ-chain unfold call counts, enabled by
/// setting `TUNGSTEN_MU_UNFOLD_STATS`. Returns `None` (a single `OnceLock`
/// check) when the env var is unset, so production builds pay ~nothing.
fn mu_unfold_stats() -> Option<&'static Mutex<HashMap<String, u64>>> {
    static STATS: OnceLock<Option<Mutex<HashMap<String, u64>>>> = OnceLock::new();
    STATS
        .get_or_init(|| {
            std::env::var_os("TUNGSTEN_MU_UNFOLD_STATS").map(|_| Mutex::new(HashMap::new()))
        })
        .as_ref()
}

/// The names of a nested μ-type's binder chain joined with `→`
/// (e.g. `α_Expr→α_Stmt`) — a compact, SCC-unique stats key.
fn mu_binder_chain(ty: &Type) -> String {
    let mut names: Vec<&str> = Vec::new();
    let mut current = ty;
    while let Type::Mu(var, body) = current {
        names.push(var.as_str());
        current = body;
    }
    names.join("→")
}

impl<'ctx> CodeGen<'ctx> {
    /// Heap-allocate a value and return a pointer to it.
    ///
    /// This is the core of `fold` for μ-types: allocate the inner struct
    /// on the heap and return an opaque pointer.
    ///
    /// # Arguments
    /// * `value` - The value to heap-allocate
    /// * `ty` - The LLVM type of the value (used for size calculation)
    /// * `alignment` - Alignment for the store (typically 16 for ARM64)
    ///
    /// # Returns
    /// A pointer to the heap-allocated value.
    pub(crate) fn fold_to_heap(
        &mut self,
        value: BasicValueEnum<'ctx>,
        ty: BasicTypeEnum<'ctx>,
        alignment: u32,
    ) -> Result<PointerValue<'ctx>, CodeGenError> {
        // Calculate size of the struct
        let size = self.type_size_bytes(ty);
        let i64_type = self.context.i64_type();
        let size_val = i64_type.const_int(size, false);

        // Allocate on heap (uses class-tagged profiling wrapper when
        // --alloc-profile is enabled)
        let ptr = self.build_malloc_call(size_val, crate::codegen::AllocClass::Mu, "mu_alloc")?;

        // Store the value with specified alignment
        let store = self
            .builder
            .build_store(ptr, value)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(alignment);

        Ok(ptr)
    }

    /// Stack-allocate a value and return a pointer to it (ADR 8.5.26d).
    ///
    /// Same semantics as `fold_to_heap` but uses alloca instead of malloc.
    /// Only safe when escape analysis proves the pointer does not outlive
    /// the current stack frame.
    pub(crate) fn fold_to_stack(
        &mut self,
        value: BasicValueEnum<'ctx>,
        ty: BasicTypeEnum<'ctx>,
        alignment: u32,
    ) -> Result<PointerValue<'ctx>, CodeGenError> {
        let ptr = self
            .builder
            .build_alloca(ty, "mu_stack")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        let store = self
            .builder
            .build_store(ptr, value)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(alignment);

        Ok(ptr)
    }

    /// Load a value from a μ-type pointer.
    ///
    /// This is the core of `unfold`: dereference the pointer to get
    /// the underlying struct value.
    ///
    /// # Arguments
    /// * `ptr` - Pointer to the heap-allocated value
    /// * `inner_ty` - The LLVM type to load
    /// * `alignment` - Alignment for the load (typically 16 for ARM64)
    /// * `name` - Name for the loaded value (for LLVM IR readability)
    ///
    /// # Returns
    /// The loaded value.
    pub(crate) fn load_mu_value(
        &mut self,
        ptr: PointerValue<'ctx>,
        inner_ty: BasicTypeEnum<'ctx>,
        alignment: u32,
        name: &str,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let loaded = self
            .builder
            .build_load(inner_ty, ptr, name)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        if let Some(inst) = loaded.as_instruction_value() {
            let _ = inst.set_alignment(alignment);
        }

        Ok(loaded)
    }
}

/// Unwrap a μ-type to get its underlying type with the μ-variables substituted.
///
/// For μ X. F[X], returns F[μ X. F[X]] (the unfolding). Nested binder
/// chains (mutually recursive SCCs) are unfolded in one simultaneous
/// pass by `tungsten_core::types::unfold_mu_type` (ADR 7.7.26k) — the
/// previous accumulated-substitution loop materialized exponentially
/// sized trees. For non-μ types, returns the type unchanged.
pub(crate) fn unwrap_mu_type(ty: &Type) -> Type {
    if matches!(ty, Type::Mu(_, _)) {
        if let Some(stats) = mu_unfold_stats() {
            return unfold_mu_with_stats(ty, stats);
        }
    }
    tungsten_core::types::unfold_mu_type(ty)
}

/// ADR 7.7.26k sizing experiment: measure input/output node counts and
/// wall time once per distinct μ-binder chain, and count every repeat
/// unfold of the same chain. One stderr line per call, aggregated
/// offline. Only reached when `TUNGSTEN_MU_UNFOLD_STATS` is set.
fn unfold_mu_with_stats(ty: &Type, stats: &'static Mutex<HashMap<String, u64>>) -> Type {
    let key = mu_binder_chain(ty);
    let repeat_count = {
        let mut map = stats.lock().unwrap();
        let calls = map.entry(key.clone()).or_insert(0);
        *calls += 1;
        *calls
    }; // lock released before the unfold work below
    if repeat_count > 1 {
        eprintln!("[mu-unfold-stats] key={key} call={repeat_count}");
        return tungsten_core::types::unfold_mu_type(ty);
    }
    let input_nodes = ty.node_count();
    let started = std::time::Instant::now();
    let unfolded = tungsten_core::types::unfold_mu_type(ty);
    let unfold_ms = started.elapsed().as_millis();
    eprintln!(
        "[mu-unfold-stats] key={key} call=1 input_nodes={input_nodes} \
         output_nodes={} output_depth={} first_unfold_ms={unfold_ms}",
        unfolded.node_count(),
        unfolded.depth(),
    );
    unfolded
}
