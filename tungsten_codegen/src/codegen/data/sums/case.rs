//! Case analysis compilation for sum types.
//!
//! Extracted from sums/mod.rs — contains `compile_case` and all supporting
//! helpers for compiling case/match on sum type values.

use super::CaseBranch;
use crate::codegen::backend::CodeGenError;
use crate::codegen::data::mu_types::unwrap_mu_type;
use crate::codegen::exec::merge::{plan_merge, MergeArm};
use crate::codegen::CodeGen;
use inkwell::values::{BasicValue, BasicValueEnum};
use inkwell::IntPredicate;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile case analysis on sum type.
    ///
    /// Sum type layout: { i32 tag, `largest_variant_type` }
    /// - Extract tag (index 0)
    /// - Load payload from data field with appropriate type cast
    ///
    /// The branch merge routes through the shared planner (ADR 2.7.26b T2):
    /// a musttail-terminated branch is excluded from the phi entirely, and
    /// both reachable branches must agree on the result type (the previous
    /// size-max pick could silently mask a lowering divergence).
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
        let left_arm = self.compile_case_arm(left_bb, data_ptr, &left_ty, left.var, left.body)?;

        // Compile right branch
        self.compilation.in_tail_position = is_tail;
        let right_arm =
            self.compile_case_arm(right_bb, data_ptr, &right_ty, right.var, right.body)?;

        // Plan + build the merge via the shared planner (ADR 2.7.26b T2).
        let merge_arms = [left_arm, right_arm];
        let site = format!("sum case in `{}`", self.current_fn_name());
        let plan = plan_merge(&merge_arms, None, &site, Some(&mut self.types))?;

        for arm in &merge_arms {
            self.terminate_merge_arm(arm, merge_bb)?;
        }

        let placeholder_ty = merge_arms[0].value.get_type();
        self.build_planned_merge(merge_bb, &plan, placeholder_ty, "case_result")
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
        let unwrapped_scrut_ty = unwrap_mu_type(&scrut_ty);
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

    /// Compile a single case arm: load payload, bind variable, compile body,
    /// restore env. Reachability follows the ADT-match convention: a musttail
    /// self-tail arm ends in the epilogue's fresh dead block (no predecessors)
    /// and its value is only a typed dummy (ADR 2.7.26b T2). The arm's block
    /// is NOT terminated here — `terminate_merge_arm` does that after planning.
    fn compile_case_arm(
        &mut self,
        bb: inkwell::basic_block::BasicBlock<'ctx>,
        data_ptr: inkwell::values::PointerValue<'ctx>,
        payload_ty: &Type,
        var_name: &str,
        body: &Term,
    ) -> Result<MergeArm<'ctx>, CodeGenError> {
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
        // Best-effort arm result type for merge-error self-decoding (ADR
        // 12.7.26c P2) — inferred while the arm binding is still in scope.
        let source_ty = self.infer_term_type(body).ok();
        if let Some(v) = old_binding {
            self.compilation.env.insert(var_name.to_string(), v);
        } else {
            self.compilation.env.remove(var_name);
        }

        let end_bb = self.builder.get_insert_block().unwrap();
        let reachable = end_bb.get_first_use().is_some();
        Ok(MergeArm {
            value: result,
            end_bb,
            reachable,
            source_ty,
        })
    }
}
