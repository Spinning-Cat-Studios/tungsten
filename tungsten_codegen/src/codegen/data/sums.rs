//! Sum type compilation - inl, inr, case, fold, unfold.
//!
//! # Sum type representation
//!
//! Sum types `A + B` are represented as `{ i32 tag, [i8 × N] data }` where:
//! - `tag`: 0 = left (A), 1 = right (B)
//! - `data`: byte array large enough to hold either A or B
//!
//! This byte-array approach is more memory-efficient than storing both variants.
//! We use alloca + store/load + bitcast to access the typed payload.
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

use crate::codegen::error::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicValue, BasicValueEnum};
use inkwell::IntPredicate;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile injection into left of sum type: inl[A + B](a) -> A + B
    ///
    /// Sum type layout: { i32 tag, [i8 × N] data }
    /// - Set tag = 0
    /// - Store 'a' into data via pointer cast
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
                .map(|p| p.into());
        }

        Ok(sum_val)
    }

    /// Compile injection into right of sum type: inr[A + B](b) -> A + B
    ///
    /// Sum type layout: { i32 tag, [i8 × N] data }
    /// - Set tag = 1
    /// - Store 'b' into data via pointer cast
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
                .map(|p| p.into());
        }

        Ok(sum_val)
    }

    /// Compile case analysis on sum type.
    ///
    /// Sum type layout: { i32 tag, [i8 × N] data }
    /// - Extract tag (index 0)
    /// - Load payload from data field with appropriate type cast
    pub(crate) fn compile_case(
        &mut self,
        scrut: &Term,
        x: &str,
        left: &Term,
        y: &str,
        right: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Prepare scrutinee: compile, infer type, unfold if μ-type
        let (sum_val, actual_scrut_ty, sum_llvm_ty) = self.prepare_case_scrutinee(scrut)?;

        // Extract left/right types from the sum
        let (left_ty, right_ty) = self.extract_sum_variant_types(&actual_scrut_ty)?;

        // Store scrutinee on stack and extract tag + data pointer
        let (tag, data_ptr) = self.store_and_extract_sum_fields(sum_val, sum_llvm_ty)?;

        // Create basic blocks for branching
        let function = self
            .current_fn
            .ok_or_else(|| CodeGenError::LlvmError("no current function".to_string()))?;
        let left_bb = self.context.append_basic_block(function, "case_left");
        let right_bb = self.context.append_basic_block(function, "case_right");
        let merge_bb = self.context.append_basic_block(function, "case_merge");

        // Branch on tag (0 = left, 1 = right)
        self.build_case_branch(tag, left_bb, right_bb)?;

        // Compile left branch
        let (left_result, left_bb_end) =
            self.compile_case_arm(left_bb, data_ptr, &left_ty, x, left)?;
        let result_type = left_result.get_type();

        // Compile right branch
        let (right_result, _right_bb_end) =
            self.compile_case_arm(right_bb, data_ptr, &right_ty, y, right)?;

        // Cast right result if types don't match
        let right_result = self.cast_branch_result(right_result, result_type)?;
        let right_bb_end = self.builder.get_insert_block().unwrap();

        // Add branch terminators and build merge phi
        self.finalize_case_branches(
            left_bb_end,
            right_bb_end,
            merge_bb,
            left_result,
            right_result,
            result_type,
        )
    }

    /// Prepare the case scrutinee: compile, infer type, unwrap μ-type if needed.
    fn prepare_case_scrutinee(
        &mut self,
        scrut: &Term,
    ) -> Result<
        (
            inkwell::values::StructValue<'ctx>,
            Type,
            inkwell::types::StructType<'ctx>,
        ),
        CodeGenError,
    > {
        let scrut_val = self.compile_term(scrut)?;
        let scrut_ty = self.infer_term_type(scrut)?;

        // Unwrap μ-type if present to get the actual sum type
        let unwrapped_scrut_ty = self.unwrap_mu_type(&scrut_ty);
        let actual_scrut_ty = self
            .types
            .expand_type(&unwrapped_scrut_ty)
            .unwrap_or(unwrapped_scrut_ty);

        let sum_llvm_ty = self.types.lower_type(&actual_scrut_ty).into_struct_type();

        // If scrutinee is μ-type, unfold it (load from pointer)
        let sum_val = if matches!(scrut_ty, Type::Mu(_, _)) {
            let ptr = scrut_val.into_pointer_value();
            let loaded = self
                .builder
                .build_load(sum_llvm_ty, ptr, "unfolded")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            if let Some(inst) = loaded.as_instruction_value() {
                let _ = inst.set_alignment(16);
            }
            loaded.into_struct_value()
        } else {
            scrut_val.into_struct_value()
        };

        Ok((sum_val, actual_scrut_ty, sum_llvm_ty))
    }

    /// Extract left and right types from a sum type.
    fn extract_sum_variant_types(&self, sum_ty: &Type) -> Result<(Type, Type), CodeGenError> {
        match sum_ty {
            Type::Sum(l, r) => Ok((l.as_ref().clone(), r.as_ref().clone())),
            _ => Err(CodeGenError::TypeError("case on non-sum type".to_string())),
        }
    }

    /// Store sum value on stack and extract tag + data pointer.
    fn store_and_extract_sum_fields(
        &mut self,
        sum_val: inkwell::values::StructValue<'ctx>,
        sum_llvm_ty: inkwell::types::StructType<'ctx>,
    ) -> Result<
        (
            inkwell::values::IntValue<'ctx>,
            inkwell::values::PointerValue<'ctx>,
        ),
        CodeGenError,
    > {
        let i32_type = self.context.i32_type();

        // Allocate sum struct on stack with 16-byte alignment
        let sum_ptr = self
            .builder
            .build_alloca(sum_llvm_ty, "case_sum_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = sum_ptr.as_instruction() {
            let _ = inst.set_alignment(16);
        }
        let store = self
            .builder
            .build_store(sum_ptr, sum_val)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(16);

        // Extract tag (field 0)
        let tag_ptr = self
            .builder
            .build_struct_gep(sum_llvm_ty, sum_ptr, 0, "tag_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let tag = self
            .builder
            .build_load(i32_type, tag_ptr, "tag")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();

        // Get pointer to data field (field 1)
        let data_ptr = self
            .builder
            .build_struct_gep(sum_llvm_ty, sum_ptr, 1, "data_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok((tag, data_ptr))
    }

    /// Build conditional branch on tag value (0 = left, 1 = right).
    fn build_case_branch(
        &mut self,
        tag: inkwell::values::IntValue<'ctx>,
        left_bb: inkwell::basic_block::BasicBlock<'ctx>,
        right_bb: inkwell::basic_block::BasicBlock<'ctx>,
    ) -> Result<(), CodeGenError> {
        let i32_type = self.context.i32_type();
        let is_left = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                tag,
                i32_type.const_int(0, false),
                "is_left",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(is_left, left_bb, right_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(())
    }

    /// Compile a single case arm: load payload, bind variable, compile body, restore env.
    fn compile_case_arm(
        &mut self,
        bb: inkwell::basic_block::BasicBlock<'ctx>,
        data_ptr: inkwell::values::PointerValue<'ctx>,
        payload_ty: &Type,
        var_name: &str,
        body: &Term,
    ) -> Result<(BasicValueEnum<'ctx>, inkwell::basic_block::BasicBlock<'ctx>), CodeGenError> {
        self.builder.position_at_end(bb);

        // Load payload with 4-byte alignment (data is at offset 4 from struct base)
        let payload_llvm_ty = self.types.lower_type(payload_ty);
        let payload = self
            .builder
            .build_load(payload_llvm_ty, data_ptr, &format!("{}_payload", var_name))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = payload.as_instruction_value() {
            let _ = inst.set_alignment(4);
        }

        // Bind variable, compile body, restore old binding
        let old_binding = self
            .env
            .insert(var_name.to_string(), (payload, payload_ty.clone()));
        let result = self.compile_term(body)?;
        if let Some(v) = old_binding {
            self.env.insert(var_name.to_string(), v);
        } else {
            self.env.remove(var_name);
        }

        let end_bb = self.builder.get_insert_block().unwrap();
        Ok((result, end_bb))
    }

    /// Cast branch result to expected type if they don't match.
    fn cast_branch_result(
        &mut self,
        result: BasicValueEnum<'ctx>,
        expected_type: inkwell::types::BasicTypeEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        if result.get_type() != expected_type {
            self.cast_to_type(result, expected_type)
        } else {
            Ok(result)
        }
    }

    /// Finalize case branches: add terminators and build merge phi.
    fn finalize_case_branches(
        &mut self,
        left_bb_end: inkwell::basic_block::BasicBlock<'ctx>,
        right_bb_end: inkwell::basic_block::BasicBlock<'ctx>,
        merge_bb: inkwell::basic_block::BasicBlock<'ctx>,
        left_result: BasicValueEnum<'ctx>,
        right_result: BasicValueEnum<'ctx>,
        result_type: inkwell::types::BasicTypeEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Add terminator for right branch (we're positioned there after cast)
        self.builder
            .build_unconditional_branch(merge_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Add terminator for left branch
        self.builder.position_at_end(left_bb_end);
        self.builder
            .build_unconditional_branch(merge_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Build merge phi
        self.builder.position_at_end(merge_bb);
        let phi = self
            .builder
            .build_phi(result_type, "case_result")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        phi.add_incoming(&[(&left_result, left_bb_end), (&right_result, right_bb_end)]);

        Ok(phi.as_basic_value())
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

        // Get the underlying sum type (unwrap the μ)
        let inner_ty = self.unwrap_mu_type(mu_ty);
        let sum_llvm_ty = self.types.lower_type(&inner_ty);

        // Use shared μ-type helper: heap-allocate and return pointer
        self.fold_to_heap(val_compiled, sum_llvm_ty, 16)
            .map(|p| p.into())
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

        // Get the underlying sum type
        let inner_ty = self.unwrap_mu_type(mu_ty);
        let sum_llvm_ty = self.types.lower_type(&inner_ty);

        // Use shared μ-type helper: load the sum value from the pointer
        self.load_mu_value(ptr, sum_llvm_ty, 16, "unfolded")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;
    use tungsten_core::terms::Term;

    fn setup_codegen_with_function(context: &Context) -> CodeGen<'_> {
        let mut codegen = CodeGen::new(context, "test");

        // Create a simple function to provide a basic block context
        let void_type = context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        let function = codegen.module.add_function("test_fn", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        codegen.builder.position_at_end(entry);
        codegen.current_fn = Some(function);

        // Declare malloc for fold operations
        let i64_type = context.i64_type();
        let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
        let malloc_type = ptr_type.fn_type(&[i64_type.into()], false);
        codegen.module.add_function("malloc", malloc_type, None);

        codegen
    }

    #[test]
    fn test_compile_inl_nat() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create inl[Nat + Bool](42) - inject Nat into Nat + Bool sum
        let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
        let val = Term::NatLit(42);

        let result = codegen.compile_inl(&sum_ty, &val).unwrap();

        // Result should be a struct value (sum type)
        assert!(result.is_struct_value());

        // Sum type struct should have 2 fields: i32 tag + data array
        let struct_val = result.into_struct_value();
        assert_eq!(struct_val.get_type().count_fields(), 2);
    }

    #[test]
    fn test_compile_inr_bool() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create inr[Nat + Bool](true) - inject Bool into Nat + Bool sum
        let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
        let val = Term::True;

        let result = codegen.compile_inr(&sum_ty, &val).unwrap();

        // Result should be a struct value (sum type)
        assert!(result.is_struct_value());

        // Sum type struct should have 2 fields: i32 tag + data array
        let struct_val = result.into_struct_value();
        assert_eq!(struct_val.get_type().count_fields(), 2);
    }

    #[test]
    fn test_compile_inl_nested_sum() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create inl[(Nat + Bool) + String](inl[Nat + Bool](1))
        // Nested sum injection
        let inner_sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
        let _outer_sum_ty = Type::Sum(Box::new(inner_sum_ty.clone()), Box::new(Type::String));

        // First create the inner inl
        let inner_val = Term::NatLit(1);
        let inner_result = codegen.compile_inl(&inner_sum_ty, &inner_val).unwrap();
        assert!(inner_result.is_struct_value());

        // The inner result is a struct, which can then be used as payload for outer inl
        // (In practice, this would be Inl(outer_sum_ty, Box::new(Inl(inner_sum_ty, ...))))
        // For this test, just verify the struct was created correctly
        assert_eq!(
            inner_result.into_struct_value().get_type().count_fields(),
            2
        );
    }

    #[test]
    fn test_sum_type_layout_i32_tag() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create a sum type and verify its LLVM representation uses i32 tag
        let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
        let llvm_ty = codegen.types.lower_type(&sum_ty).into_struct_type();

        // Should have 2 fields
        assert_eq!(llvm_ty.count_fields(), 2);

        // First field should be i32 (tag)
        let tag_type = llvm_ty.get_field_type_at_index(0).unwrap();
        assert!(tag_type.is_int_type());
        assert_eq!(tag_type.into_int_type().get_bit_width(), 32);

        // Second field should be array (data)
        let data_type = llvm_ty.get_field_type_at_index(1).unwrap();
        assert!(data_type.is_array_type());
    }

    #[test]
    fn test_compile_case_simple() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create: case (inl[Nat + Bool] 42) of inl x => x | inr y => 0
        // This should evaluate to 42
        let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
        let scrut = Term::Inl(sum_ty.clone(), Box::new(Term::NatLit(42)));
        let left_branch = Term::Var("x".to_string());
        let right_branch = Term::NatLit(0);

        let result = codegen.compile_case(&scrut, "x", &left_branch, "y", &right_branch);

        // Should compile successfully
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_sum_variant_types_valid() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
        let result = codegen.extract_sum_variant_types(&sum_ty);

        assert!(result.is_ok());
        let (left, right) = result.unwrap();
        assert_eq!(left, Type::Nat);
        assert_eq!(right, Type::Bool);
    }

    #[test]
    fn test_extract_sum_variant_types_nested() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        // (Nat + Bool) + String
        let inner = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
        let outer = Type::Sum(Box::new(inner.clone()), Box::new(Type::String));
        let result = codegen.extract_sum_variant_types(&outer);

        assert!(result.is_ok());
        let (left, right) = result.unwrap();
        assert_eq!(left, inner);
        assert_eq!(right, Type::String);
    }

    #[test]
    fn test_extract_sum_variant_types_non_sum_error() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let nat_ty = Type::Nat;
        let result = codegen.extract_sum_variant_types(&nat_ty);

        assert!(result.is_err());
    }

    #[test]
    fn test_compile_case_right_branch() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create: case (inr[Nat + Bool] true) of inl x => 0 | inr y => 1
        // Tests right branch compilation
        let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
        let scrut = Term::Inr(sum_ty.clone(), Box::new(Term::True));
        let left_branch = Term::NatLit(0);
        let right_branch = Term::NatLit(1);

        let result = codegen.compile_case(&scrut, "x", &left_branch, "y", &right_branch);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compile_case_nested_sum() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Test case on nested sum: (Unit + Nat) + Bool
        let inner_sum = Type::Sum(Box::new(Type::Unit), Box::new(Type::Nat));
        let outer_sum = Type::Sum(Box::new(inner_sum.clone()), Box::new(Type::Bool));

        // inl[inner_sum + Bool](inl[Unit + Nat](()))
        let inner_scrut = Term::Inl(inner_sum.clone(), Box::new(Term::Unit));
        let scrut = Term::Inl(outer_sum.clone(), Box::new(inner_scrut));

        // Both branches return Nat
        let left_branch = Term::NatLit(42);
        let right_branch = Term::NatLit(0);

        let result = codegen.compile_case(&scrut, "x", &left_branch, "y", &right_branch);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compile_case_with_variable_binding() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Test that variable binding works: inl x => x uses the bound value
        let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Nat));
        let scrut = Term::Inl(sum_ty.clone(), Box::new(Term::NatLit(99)));

        // Both branches use their bound variable
        let left_branch = Term::Var("x".to_string());
        let right_branch = Term::Var("y".to_string());

        let result = codegen.compile_case(&scrut, "x", &left_branch, "y", &right_branch);
        assert!(result.is_ok());
    }
}
