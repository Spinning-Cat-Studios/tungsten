//! Flat ADT compilation - direct tag + payload representation (ADR 2.2.26).
//!
//! # Flat ADT representation
//!
//! ADT types with n≥3 constructors are represented as `{ i32 tag, [max_payload x i8] data }`:
//! - `tag`: Constructor index (0, 1, 2, ..., n-1)
//! - `data`: Byte array large enough to hold the largest payload
//!
//! This provides O(1) variant dispatch via LLVM `switch` instruction, avoiding the
//! O(n) nested case expressions from right-nested binary sums.
//!
//! # Why i32 tag?
//!
//! Using i32 (vs i8) ensures:
//! - Support for ADTs with >256 constructors
//! - Natural alignment on modern architectures
//! - Consistent with Rust's enum discriminant
//!
//! # Module structure
//!
//! - `mod.rs`: Constructor compilation + type helpers + trace instrumentation
//! - `adt_match.rs`: Pattern matching compilation (switch-based dispatch)

mod adt_match;

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicValue, BasicValueEnum};
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile ADT constructor application: `AdtConstruct(adt_ty`, `variant_idx`, payload)
    ///
    /// Generates:
    /// 1. Allocate `{ i32, [max_payload x i8] }` on stack
    /// 2. Store tag = `variant_idx`
    /// 3. Store payload into data field (bitcast to payload type)
    /// 4. Load and return the complete struct
    ///
    /// NOTE: This function NEVER folds/heap-allocates. For recursive ADTs, the
    /// elaborator wraps constructor calls in `Term::Fold`, which handles heap
    /// allocation via `compile_fold`. `compile_adt_construct` only builds the struct value.
    pub(crate) fn compile_adt_construct(
        &mut self,
        adt_ty: &Type,
        variant_idx: usize,
        payload: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Compile the payload first
        let payload_val = self.compile_term(payload)?;

        // Unwrap μ-type if present to get the actual ADT type
        // This handles recursive types like μ X. Adt(...)
        let unwrapped = self.unwrap_mu_type(adt_ty);

        // Use resolve_to_flat_adt to get the Type::Adt representation
        // (not the Sum encoding from expand_type)
        let actual_adt_ty = self.types.resolve_to_flat_adt(&unwrapped).ok_or_else(|| {
            CodeGenError::TypeError(format!(
                "compile_adt_construct requires Adt type, got {unwrapped:?} (from {adt_ty:?})"
            ))
        })?;

        // Get the ADT's LLVM struct type: { i32, [N x i8] }
        let adt_llvm_ty = self.types.lower_type(&actual_adt_ty).into_struct_type();
        let i32_type = self.context.i32_type();

        // Allocate the ADT struct on stack with 16-byte alignment for ARM64 ABI
        let adt_ptr = self
            .builder
            .build_alloca(adt_llvm_ty, "adt_alloca")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = adt_ptr.as_instruction() {
            let _ = inst.set_alignment(16);
        }

        // Store tag = variant_idx at field 0
        let tag_ptr = self
            .builder
            .build_struct_gep(adt_llvm_ty, adt_ptr, 0, "tag_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(tag_ptr, i32_type.const_int(variant_idx as u64, false))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Store payload into data field (field 1) via pointer cast
        // NOTE: data_ptr is at offset 4 from struct base, so max valid alignment is 4
        let data_ptr = self
            .builder
            .build_struct_gep(adt_llvm_ty, adt_ptr, 1, "data_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let store = self
            .builder
            .build_store(data_ptr, payload_val)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(4);

        // Load the complete struct value
        let adt_val = self
            .builder
            .build_load(adt_llvm_ty, adt_ptr, "adt_val")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = adt_val.as_instruction_value() {
            let _ = inst.set_alignment(16);
        }

        // Return the struct value directly.
        // For recursive ADTs, the elaborator wraps this in Term::Fold,
        // which calls compile_fold to handle heap allocation.

        // T3: Emit trace call if --trace-adt-ops is enabled
        if let Some(ref filter) = self.tracing.trace_adt_ops.clone() {
            let adt_name = self.adt_type_name(adt_ty);
            if filter == "all" || adt_name.contains(filter.as_str()) {
                let data_size = self.type_size_bytes(adt_llvm_ty.into());
                self.emit_trace_adt_construct(&adt_name, variant_idx, data_ptr, data_size)?;
            }
        }

        Ok(adt_val)
    }

    /// Check if a type mentions a specific named type variable (TyVar).
    /// Used to detect recursive ADTs by checking if variants reference α_{adt_name}.
    ///
    /// Note: test-only after ADR 10.5.26d extracted the shared walker into
    /// `tungsten_core::types::Type::any_tyvar`. Production code calls
    /// `any_tyvar` directly; this wrapper exists to keep existing ADT tests.
    #[cfg(test)]
    fn type_mentions_named_var(ty: &Type, var_name: &str) -> bool {
        ty.any_tyvar(&|name| name == var_name)
    }

    // -- T3: ADT trace instrumentation helpers (ADR 16.4.26a) --

    /// Extract a human-readable name from an ADT type.
    fn adt_type_name(&self, ty: &Type) -> String {
        match ty {
            Type::Adt(name, _, _) => name.clone(),
            Type::Mu(_, inner) => self.adt_type_name(inner),
            Type::App(name, _) => name.clone(),
            _ => format!("{ty:?}"),
        }
    }

    /// Emit a call to `__tungsten_trace_adt_construct(type_name, variant_idx, data_ptr, data_size)`.
    fn emit_trace_adt_construct(
        &mut self,
        type_name: &str,
        variant_idx: usize,
        data_ptr: inkwell::values::PointerValue<'ctx>,
        data_size: u64,
    ) -> Result<(), CodeGenError> {
        let trace_fn = self
            .module
            .get_function("__tungsten_trace_adt_construct")
            .ok_or_else(|| {
                CodeGenError::LlvmError(
                    "trace function __tungsten_trace_adt_construct not declared".to_string(),
                )
            })?;

        let name_str = self
            .builder
            .build_global_string_ptr(type_name, "trace_adt_name")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let idx_val = self.context.i32_type().const_int(variant_idx as u64, false);
        let size_val = self.context.i64_type().const_int(data_size, false);

        self.builder
            .build_call(
                trace_fn,
                &[
                    name_str.as_pointer_value().into(),
                    idx_val.into(),
                    data_ptr.into(),
                    size_val.into(),
                ],
                "",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok(())
    }

    /// Emit a call to `__tungsten_trace_adt_match(type_name, tag, data_ptr, data_size)`.
    fn emit_trace_adt_match(
        &mut self,
        type_name: &str,
        tag: inkwell::values::IntValue<'ctx>,
        data_ptr: inkwell::values::PointerValue<'ctx>,
        data_size: u64,
    ) -> Result<(), CodeGenError> {
        let trace_fn = self
            .module
            .get_function("__tungsten_trace_adt_match")
            .ok_or_else(|| {
                CodeGenError::LlvmError(
                    "trace function __tungsten_trace_adt_match not declared".to_string(),
                )
            })?;

        let name_str = self
            .builder
            .build_global_string_ptr(type_name, "trace_adt_name")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let size_val = self.context.i64_type().const_int(data_size, false);

        self.builder
            .build_call(
                trace_fn,
                &[
                    name_str.as_pointer_value().into(),
                    tag.into(),
                    data_ptr.into(),
                    size_val.into(),
                ],
                "",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok(())
    }
}

// Tests: see tests.rs
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
