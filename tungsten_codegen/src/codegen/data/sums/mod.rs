//! Sum type compilation - inl, inr, case, fold, unfold.
//!
//! # Type Sourcing
//!
//! This module TRUSTS type annotations on Core IR terms for layout decisions:
//! - `compile_inl`/`compile_inr`: read the `Sum(L, R)` annotation to determine payload slot sizes
//! - `compile_case`: reads the scrutinee's type annotation to compute struct layout and GEP offsets
//!
//! If type annotations carry unresolved `TyVars`, this module will produce wrong-sized LLVM structs.
//! The G1 assertion in `lower_type` guards against this; see also W2.1 (ADR 11.4.26c).
//!
//! # Sum type representation
//!
//! Sum types `A + B` are represented as `{ i32 tag, largest_variant_type }` where:
//! - `tag`: 0 = left (A), 1 = right (B)
//! - `largest_variant_type`: the concrete LLVM type of whichever variant is larger
//!
//! W4 (ADR 11.4.26c): The payload uses the larger variant's concrete type instead
//! of an opaque `[N x i8]` byte array. This gives LLVM's SROA pass type information
//! about the payload, preventing misinterpretation of bytes after scalar replacement.
//! The smaller variant is stored/loaded through a pointer cast at the same offset.
//!
//! The i32 tag matches ADT representation for consistency across all sum-like types.
//!
//! # μ-type handling
//!
//! Recursive types like `μ X. 1 + X` are represented as opaque pointers (`i8*`)
//! at the LLVM level. The actual data is heap-allocated as the underlying sum type.
//!
//! - `fold` allocates the sum struct on the heap and returns a pointer
//! - `unfold` dereferences the pointer to get the sum struct
//!
//! This design intentionally leaks memory for simplicity. A borrow checker
//! (planned for Phase 3) will provide proper ownership tracking.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicValue, BasicValueEnum};
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

/// A case branch binding: the variable name and the body expression.
pub(crate) struct CaseBranch<'a> {
    pub var: &'a str,
    pub body: &'a Term,
}

mod case;

impl<'ctx> CodeGen<'ctx> {
    /// Compile injection into left of sum type: inl[A + B](a) -> A + B
    ///
    /// Sum type layout: { i32 tag, `largest_variant_type` }
    /// - Set tag = 0
    /// - Store 'a' into data field via pointer cast
    pub(crate) fn compile_inl(
        &mut self,
        sum_ty: &Type,
        val: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Unwrap μ-type if present to get the actual sum type
        let unwrapped = self.unwrap_mu_type(sum_ty);
        // Expand ADT types (Type::App) to their sum form
        let actual_sum_ty = self.types.expand_type(&unwrapped).unwrap_or(unwrapped);

        let compiled_val = self.compile_term(val)?;
        let sum_llvm_ty = self.types.lower_type(&actual_sum_ty).into_struct_type();
        let i32_type = self.context.i32_type();

        // Allocate the sum struct on stack with 16-byte alignment for ARM64 ABI
        let sum_ptr = self
            .builder
            .build_alloca(sum_llvm_ty, "sum_alloca")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = sum_ptr.as_instruction() {
            let _ = inst.set_alignment(16);
        }

        // Store tag = 0 (left)
        let tag_ptr = self
            .builder
            .build_struct_gep(sum_llvm_ty, sum_ptr, 0, "tag_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(tag_ptr, i32_type.const_int(0, false))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Get pointer to data field, cast to payload type, and store value
        let data_ptr = self
            .builder
            .build_struct_gep(sum_llvm_ty, sum_ptr, 1, "data_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        // data_ptr is ptr to [i8 × N], we can store directly through it as payload type
        // NOTE: data_ptr is at offset 4 from struct base, so max valid alignment is 4
        // Using align 16 here would cause misaligned access on ARM64 (SIGSEGV)
        let store = self
            .builder
            .build_store(data_ptr, compiled_val)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(4);

        // Load the complete struct with proper alignment
        let sum_val = self
            .builder
            .build_load(sum_llvm_ty, sum_ptr, "sum_val")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = sum_val.as_instruction_value() {
            let _ = inst.set_alignment(16);
        }

        // If original type was Mu-wrapped (recursive), fold by heap-allocating and returning pointer
        if matches!(sum_ty, Type::Mu(_, _)) {
            return self
                .fold_to_heap(sum_val, sum_llvm_ty.into(), 16)
                .map(std::convert::Into::into);
        }

        Ok(sum_val)
    }

    /// Compile injection into right of sum type: inr[A + B](b) -> A + B
    ///
    /// Sum type layout: { i32 tag, `largest_variant_type` }
    /// - Set tag = 1
    /// - Store 'b' into data field via pointer cast
    pub(crate) fn compile_inr(
        &mut self,
        sum_ty: &Type,
        val: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Unwrap μ-type if present to get the actual sum type
        let unwrapped = self.unwrap_mu_type(sum_ty);
        // Expand ADT types (Type::App) to their sum form
        let actual_sum_ty = self.types.expand_type(&unwrapped).unwrap_or(unwrapped);

        let compiled_val = self.compile_term(val)?;
        let sum_llvm_ty = self.types.lower_type(&actual_sum_ty).into_struct_type();
        let i32_type = self.context.i32_type();

        // Allocate the sum struct on stack with 16-byte alignment for ARM64 ABI
        let sum_ptr = self
            .builder
            .build_alloca(sum_llvm_ty, "sum_alloca")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = sum_ptr.as_instruction() {
            let _ = inst.set_alignment(16);
        }

        // Store tag = 1 (right)
        let tag_ptr = self
            .builder
            .build_struct_gep(sum_llvm_ty, sum_ptr, 0, "tag_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(tag_ptr, i32_type.const_int(1, false))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Get pointer to data field and store value
        // NOTE: data_ptr is at offset 4 from struct base, so max valid alignment is 4
        // Using align 16 here would cause misaligned access on ARM64 (SIGSEGV)
        let data_ptr = self
            .builder
            .build_struct_gep(sum_llvm_ty, sum_ptr, 1, "data_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let store = self
            .builder
            .build_store(data_ptr, compiled_val)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(4);

        // Load the complete struct with proper alignment
        let sum_val = self
            .builder
            .build_load(sum_llvm_ty, sum_ptr, "sum_val")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = sum_val.as_instruction_value() {
            let _ = inst.set_alignment(16);
        }

        // If original type was Mu-wrapped (recursive), fold by heap-allocating and returning pointer
        if matches!(sum_ty, Type::Mu(_, _)) {
            return self
                .fold_to_heap(sum_val, sum_llvm_ty.into(), 16)
                .map(std::convert::Into::into);
        }

        Ok(sum_val)
    }

    /// Compile fold: introduce a μ-type by heap-allocating the underlying sum.
    ///
    /// fold[μ X. F[X]](v : F[μ X. F[X]]) -> μ X. F[X]
    ///
    /// At runtime: allocate F[μ X. F[X]] on heap, return pointer.
    pub(crate) fn compile_fold(
        &mut self,
        mu_ty: &Type,
        val: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let val_compiled = self.compile_term(val)?;

        // Get the underlying sum type (unwrap all nested μ layers for mutual recursion)
        let inner_ty = self.unwrap_mu_type(mu_ty);
        let sum_llvm_ty = self.types.lower_type(&inner_ty);

        // Check if this fold is in a let-binding that escape analysis marked as non-escaping
        let use_stack = self
            .naming
            .current_binding_name
            .as_ref()
            .is_some_and(|name| self.defs.non_escaping_folds.contains(name));

        if self.tracing.trace_escape {
            let binding = self
                .naming
                .current_binding_name
                .as_deref()
                .unwrap_or("<anon>");
            if use_stack {
                eprintln!("[escape] {binding}: STACK (non-escaping fold)");
            } else {
                eprintln!("[escape] {binding}: HEAP (escaping or unknown)");
            }
        }

        if use_stack {
            self.fold_to_stack(val_compiled, sum_llvm_ty, 16)
                .map(std::convert::Into::into)
        } else {
            self.fold_to_heap(val_compiled, sum_llvm_ty, 16)
                .map(std::convert::Into::into)
        }
    }

    /// Compile unfold: eliminate a μ-type by dereferencing the pointer.
    ///
    /// unfold[μ X. F[X]](v : μ X. F[X]) -> F[μ X. F[X]]
    ///
    /// At runtime: dereference the pointer to get the sum struct.
    pub(crate) fn compile_unfold(
        &mut self,
        mu_ty: &Type,
        val: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let ptr = self.compile_term(val)?.into_pointer_value();

        // Get the underlying sum type (unwrap all nested μ layers for mutual recursion)
        let inner_ty = self.unwrap_mu_type(mu_ty);
        let sum_llvm_ty = self.types.lower_type(&inner_ty);

        // Use shared μ-type helper: load the sum value from the pointer
        self.load_mu_value(ptr, sum_llvm_ty, 16, "unfolded")
    }
}

#[cfg(test)]
mod tests;
