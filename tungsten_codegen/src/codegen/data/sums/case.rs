//! Case analysis compilation for sum types.
//!
//! Extracted from sums/mod.rs — contains `compile_case` and all supporting
//! helpers for compiling case/match on sum type values.

use super::CaseBranch;
use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicValue, BasicValueEnum};
use inkwell::IntPredicate;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

/// Result of compiling a single case branch: the LLVM value and ending basic block.
struct BranchResult<'ctx> {
    bb_end: inkwell::basic_block::BasicBlock<'ctx>,
    result: BasicValueEnum<'ctx>,
}

impl<'ctx> CodeGen<'ctx> {
    /// Compile case analysis on sum type.
    ///
    /// Sum type layout: { i32 tag, `largest_variant_type` }
    /// - Extract tag (index 0)
    /// - Load payload from data field with appropriate type cast
    pub(crate) fn compile_case(
        &mut self,
        scrut: &Term,
        left: &CaseBranch<'_>,
        right: &CaseBranch<'_>,
        is_tail: bool,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Prepare scrutinee: compile, infer type, unfold if μ-type
        let (sum_val, actual_scrut_ty, sum_llvm_ty) = self.prepare_case_scrutinee(scrut)?;

        // Extract left/right types from the sum
        let (left_ty, right_ty) = self.extract_sum_variant_types(&actual_scrut_ty)?;

        // Store scrutinee on stack and extract tag + data pointer
        let (tag, data_ptr) = self.store_and_extract_sum_fields(sum_val, sum_llvm_ty)?;

        // Create basic blocks for branching
        let function = self
            .compilation
            .current_fn
            .ok_or_else(|| CodeGenError::LlvmError("no current function".to_string()))?;
        let left_bb = self.context.append_basic_block(function, "case_left");
        let right_bb = self.context.append_basic_block(function, "case_right");
        let merge_bb = self.context.append_basic_block(function, "case_merge");

        // Branch on tag (0 = left, 1 = right)
        self.build_case_branch(tag, left_bb, right_bb)?;

        // Compile left branch
        self.compilation.in_tail_position = is_tail;
        let (left_result, left_bb_end) =
            self.compile_case_arm(left_bb, data_ptr, &left_ty, left.var, left.body)?;

        // Compile right branch
        self.compilation.in_tail_position = is_tail;
        let (right_result, right_bb_end) =
            self.compile_case_arm(right_bb, data_ptr, &right_ty, right.var, right.body)?;

        // Use the LARGER type as the merge type to avoid truncation.
        let left_size = self.type_size_bytes(left_result.get_type());
        let right_size = self.type_size_bytes(right_result.get_type());
        let result_type = if right_size > left_size {
            right_result.get_type()
        } else {
            left_result.get_type()
        };

        // Cast both results to the merge type (no-op for the one that's already correct).
        self.builder.position_at_end(left_bb_end);
        let left_result = self.cast_branch_result(left_result, result_type)?;
        let left_bb_end = self.builder.get_insert_block().unwrap();

        self.builder.position_at_end(right_bb_end);
        let right_result = self.cast_branch_result(right_result, result_type)?;
        let right_bb_end = self.builder.get_insert_block().unwrap();

        // Add branch terminators and build merge phi
        let left_br = BranchResult {
            bb_end: left_bb_end,
            result: left_result,
        };
        let right_br = BranchResult {
            bb_end: right_bb_end,
            result: right_result,
        };
        self.finalize_case_branches(&left_br, &right_br, merge_bb, result_type)
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

        // Unwrap ALL μ-type layers if present to get the actual sum type.
        // unwrap_mu_type handles nested Mu binders for mutual recursion.
        let unwrapped_scrut_ty = self.unwrap_mu_type(&scrut_ty);
        let is_mu = matches!(scrut_ty, Type::Mu(_, _));
        let actual_scrut_ty = self
            .types
            .expand_type(&unwrapped_scrut_ty)
            .unwrap_or(unwrapped_scrut_ty);

        let sum_llvm_ty = self.types.lower_type(&actual_scrut_ty).into_struct_type();

        // If scrutinee is μ-type, unfold it (load from pointer)
        let sum_val = if is_mu {
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
    pub(crate) fn extract_sum_variant_types(
        &self,
        sum_ty: &Type,
    ) -> Result<(Type, Type), CodeGenError> {
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
            .build_load(payload_llvm_ty, data_ptr, &format!("{var_name}_payload"))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = payload.as_instruction_value() {
            let _ = inst.set_alignment(4);
        }

        // Bind variable, compile body, restore old binding
        let old_binding = self
            .compilation
            .env
            .insert(var_name.to_string(), (payload, payload_ty.clone()));
        let result = self.compile_term(body)?;
        if let Some(v) = old_binding {
            self.compilation.env.insert(var_name.to_string(), v);
        } else {
            self.compilation.env.remove(var_name);
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
        if result.get_type() == expected_type {
            Ok(result)
        } else {
            self.cast_to_type(result, expected_type)
        }
    }

    /// Finalize case branches: add terminators and build merge phi.
    fn finalize_case_branches(
        &mut self,
        left: &BranchResult<'ctx>,
        right: &BranchResult<'ctx>,
        merge_bb: inkwell::basic_block::BasicBlock<'ctx>,
        result_type: inkwell::types::BasicTypeEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Add terminator for right branch (we're positioned there after cast)
        self.builder
            .build_unconditional_branch(merge_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Add terminator for left branch
        self.builder.position_at_end(left.bb_end);
        self.builder
            .build_unconditional_branch(merge_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Build merge phi
        self.builder.position_at_end(merge_bb);
        let phi = self
            .builder
            .build_phi(result_type, "case_result")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        phi.add_incoming(&[(&left.result, left.bb_end), (&right.result, right.bb_end)]);

        Ok(phi.as_basic_value())
    }
}
