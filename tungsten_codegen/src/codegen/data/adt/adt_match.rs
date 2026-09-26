//! ADT pattern matching compilation — LLVM `switch`-based dispatch.
//!
//! Generates tag extraction + switch + per-arm payload loading for flat ADT types.

use crate::codegen::backend::CodeGenError;
use crate::codegen::data::mu_types::unwrap_mu_type;
use crate::codegen::exec::merge::{plan_merge, MergeArm};
use crate::codegen::CodeGen;
use inkwell::values::{BasicValue, BasicValueEnum};
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

/// Shared context for compiling ADT match arms.
///
/// Bundles the data pointer, variant type info, merge block,
/// and tail-position flag common to all arms.
struct AdtArmCtx<'ctx> {
    data_ptr: inkwell::values::PointerValue<'ctx>,
    variants: Vec<(String, Type)>,
    merge_bb: inkwell::basic_block::BasicBlock<'ctx>,
    is_tail: bool,
}

impl<'ctx> CodeGen<'ctx> {
    /// Compile ADT pattern matching: AdtMatch(scrutinee, arms)
    ///
    /// Each arm is (`variant_idx`, `var_name`, body).
    ///
    /// Generates:
    /// 1. Extract tag from scrutinee
    /// 2. Build LLVM `switch` on tag value
    /// 3. For each arm: load payload, bind variable, compile body
    /// 4. Merge results via phi node
    pub(crate) fn compile_adt_match(
        &mut self,
        scrutinee: &Term,
        arms: &[(usize, String, Box<Term>)],
        is_tail: bool,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // scrutinee is NOT in tail position (in_tail_position already false)
        // Prepare scrutinee: compile, infer type, unwrap μ-type if needed
        let scrut_val = self.compile_term(scrutinee)?;
        let scrut_ty = self.infer_term_type(scrutinee)?;
        let (actual_scrut_ty, scrut_struct) = self.unwrap_adt_scrutinee(&scrut_ty, scrut_val)?;

        // Get ADT layout info
        let adt_llvm_ty = self.types.lower_type(&actual_scrut_ty).into_struct_type();
        let variants = self.get_adt_variants(&actual_scrut_ty)?;

        // Store scrutinee and extract tag + data pointer
        let (tag, data_ptr) = self.store_and_extract_adt_fields(scrut_struct, adt_llvm_ty)?;

        // T3: Emit trace call if --trace-adt-ops is enabled
        if let Some(ref filter) = self.tracing.trace_adt_ops.clone() {
            let adt_name = self.adt_type_name(&scrut_ty);
            if filter == "all" || adt_name.contains(filter.as_str()) {
                let data_size = self.type_size_bytes(adt_llvm_ty.into());
                self.emit_trace_adt_match(&adt_name, tag, data_ptr, data_size)?;
            }
        }

        // Build switch with basic blocks for each arm
        let function = self
            .compilation
            .current_fn
            .ok_or_else(|| CodeGenError::LlvmError("no current function".to_string()))?;
        let (merge_bb, switch_info) = self.build_adt_switch(function, tag, arms)?;

        // Compile each arm and collect merge arms (value, end block, reachable)
        let mut arm_ctx = AdtArmCtx {
            data_ptr,
            variants,
            merge_bb,
            is_tail,
        };
        let merge_arms = self.compile_adt_arms(&switch_info, arms, &mut arm_ctx)?;

        // Build merge block via the shared planner (ADR 2.7.26b T2)
        self.build_adt_merge_phi(merge_bb, &merge_arms)
    }

    /// Store ADT scrutinee on stack and extract tag + data pointer.
    fn store_and_extract_adt_fields(
        &mut self,
        scrut_struct: inkwell::values::StructValue<'ctx>,
        adt_llvm_ty: inkwell::types::StructType<'ctx>,
    ) -> Result<
        (
            inkwell::values::IntValue<'ctx>,
            inkwell::values::PointerValue<'ctx>,
        ),
        CodeGenError,
    > {
        // Allocate scrutinee on stack with 16-byte alignment
        let scrut_ptr = self
            .builder
            .build_alloca(adt_llvm_ty, "adt_scrut_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = scrut_ptr.as_instruction() {
            let _ = inst.set_alignment(16);
        }
        let store = self
            .builder
            .build_store(scrut_ptr, scrut_struct)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(16);

        // Extract tag (field 0)
        let i32_type = self.context.i32_type();
        let tag_ptr = self
            .builder
            .build_struct_gep(adt_llvm_ty, scrut_ptr, 0, "tag_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let tag = self
            .builder
            .build_load(i32_type, tag_ptr, "tag")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();

        // Get pointer to data field (field 1)
        let data_ptr = self
            .builder
            .build_struct_gep(adt_llvm_ty, scrut_ptr, 1, "data_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok((tag, data_ptr))
    }

    /// Build switch instruction with basic blocks for each arm.
    /// Returns the merge block and info about each arm's block.
    fn build_adt_switch(
        &mut self,
        function: inkwell::values::FunctionValue<'ctx>,
        tag: inkwell::values::IntValue<'ctx>,
        arms: &[(usize, String, Box<Term>)],
    ) -> Result<
        (
            inkwell::basic_block::BasicBlock<'ctx>,
            Vec<(inkwell::basic_block::BasicBlock<'ctx>, String, usize)>,
        ),
        CodeGenError,
    > {
        let i32_type = self.context.i32_type();
        let merge_bb = self.context.append_basic_block(function, "adt_merge");
        let default_bb = self.context.append_basic_block(function, "adt_unreachable");

        // Create blocks and switch cases for each arm
        let mut arm_blocks = Vec::with_capacity(arms.len());
        let mut switch_cases = Vec::with_capacity(arms.len());

        for (variant_idx, var_name, _) in arms {
            let bb = self
                .context
                .append_basic_block(function, &format!("adt_case_{variant_idx}"));
            arm_blocks.push((bb, var_name.clone(), *variant_idx));
            switch_cases.push((i32_type.const_int(*variant_idx as u64, false), bb));
        }

        // Build switch instruction
        self.builder
            .build_switch(tag, default_bb, &switch_cases)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Default case: unreachable (exhaustive match)
        self.builder.position_at_end(default_bb);
        self.builder
            .build_unreachable()
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok((merge_bb, arm_blocks))
    }

    /// Compile all ADT match arms into [`MergeArm`]s. An arm is unreachable
    /// when it terminated control flow itself (a `musttail … ; ret` self-tail
    /// arm, ADRs 1.7.26a/e) and its "value" is only the epilogue's dummy in an
    /// unreachable dead block.
    fn compile_adt_arms(
        &mut self,
        arm_blocks: &[(inkwell::basic_block::BasicBlock<'ctx>, String, usize)],
        arms: &[(usize, String, Box<Term>)],
        ctx: &mut AdtArmCtx<'ctx>,
    ) -> Result<Vec<MergeArm<'ctx>>, CodeGenError> {
        let mut merge_arms = Vec::with_capacity(arms.len());

        for (i, (bb, var_name, variant_idx)) in arm_blocks.iter().enumerate() {
            let arm = self.compile_adt_arm(*bb, *variant_idx, var_name, &arms[i].2, ctx)?;
            merge_arms.push(arm);
        }

        Ok(merge_arms)
    }

    /// Compile a single ADT match arm. The arm is unreachable when its end
    /// block has no predecessors — i.e. the arm body already terminated
    /// (musttail self-tail edge) and the returned value is only the epilogue
    /// dummy, which must NOT participate in result-type unification
    /// (ADR 1.7.26e §6.5: a first-arm `i1` dummy truncated real sibling-arm
    /// results through a 1-byte `cast_to_type` memcpy). The arm's block is NOT
    /// terminated here — `terminate_merge_arm` does that after planning.
    fn compile_adt_arm(
        &mut self,
        bb: inkwell::basic_block::BasicBlock<'ctx>,
        variant_idx: usize,
        var_name: &str,
        body: &Term,
        ctx: &mut AdtArmCtx<'ctx>,
    ) -> Result<MergeArm<'ctx>, CodeGenError> {
        self.builder.position_at_end(bb);

        // Get payload type for this variant
        let payload_ty = ctx
            .variants
            .get(variant_idx)
            .map_or(Type::Unit, |(_, ty)| ty.clone());
        let payload_llvm_ty = self.types.lower_type(&payload_ty);

        // Load payload with 4-byte alignment (data is at offset 4)
        let payload_val = self
            .builder
            .build_load(
                payload_llvm_ty,
                ctx.data_ptr,
                &format!("payload_{variant_idx}"),
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = payload_val.as_instruction_value() {
            let _ = inst.set_alignment(4);
        }

        // Bind variable, compile body, restore env
        let old_binding = self
            .compilation
            .env
            .insert(var_name.to_string(), (payload_val, payload_ty));
        // Arm body IS in tail position if the match is
        self.compilation.in_tail_position = ctx.is_tail;
        let arm_result = self.compile_term(body)?;
        // Best-effort arm result type for merge-error self-decoding (ADR
        // 12.7.26c P2) — inferred while the arm binding is still in scope.
        let source_ty = self.infer_term_type(body).ok();
        if let Some(v) = old_binding {
            self.compilation.env.insert(var_name.to_string(), v);
        } else {
            self.compilation.env.remove(var_name);
        }

        // Reachability: a musttail self-tail arm ends in the epilogue's fresh
        // dead block, which nothing branches to. Its value is a typed dummy
        // and control never flows from it to the merge.
        let actual_end_bb = self.builder.get_insert_block().unwrap();
        let reachable = actual_end_bb.get_first_use().is_some();

        Ok(MergeArm {
            value: arm_result,
            end_bb: actual_end_bb,
            reachable,
            source_ty,
        })
    }

    /// Terminate the arm blocks and build the merge via the shared planner
    /// (ADR 2.7.26b T2): unreachable (musttail-terminated) arms are excluded
    /// from the phi ENTIRELY — their dead blocks end in `unreachable`, not a
    /// branch; all reachable arms must agree on the result type (hard error
    /// otherwise); the all-unreachable case emits no phi at all.
    fn build_adt_merge_phi(
        &mut self,
        merge_bb: inkwell::basic_block::BasicBlock<'ctx>,
        merge_arms: &[MergeArm<'ctx>],
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let site = format!("ADT match in `{}`", self.current_fn_name());
        let plan = plan_merge(merge_arms, None, &site, Some(&mut self.types))?;

        for arm in merge_arms {
            self.terminate_merge_arm(arm, merge_bb)?;
        }

        // Dead-merge placeholder type: the first arm's dummy type (the merge
        // block has no predecessors; anything appended to it never executes).
        let placeholder_ty = merge_arms
            .first()
            .map(|a| a.value.get_type())
            .ok_or_else(|| CodeGenError::TypeError("ADT match has no arms".to_string()))?;

        self.build_planned_merge(merge_bb, &plan, placeholder_ty, "adt_result")
    }

    /// Unwrap μ-type wrapper and return the actual ADT type + struct value.
    fn unwrap_adt_scrutinee(
        &mut self,
        ty: &Type,
        val: BasicValueEnum<'ctx>,
    ) -> Result<(Type, inkwell::values::StructValue<'ctx>), CodeGenError> {
        match ty {
            Type::Mu(_, _) => {
                // For μ X. Adt(...), unwrap ALL Mu layers and load from pointer.
                // unwrap_mu_type handles nested Mu binders for mutual recursion.
                let inner_ty = unwrap_mu_type(ty);

                // Resolve to flat ADT if inner is a type variable
                let resolved_inner = self
                    .types
                    .resolve_to_flat_adt(&inner_ty)
                    .unwrap_or(inner_ty);

                let inner_llvm_ty = self.types.lower_type(&resolved_inner);

                let ptr = val.into_pointer_value();
                // Use shared μ-type helper: load the struct value from the pointer
                let loaded = self.load_mu_value(ptr, inner_llvm_ty, 16, "unfolded_adt")?;

                Ok((resolved_inner, loaded.into_struct_value()))
            }
            Type::Adt(_, _, _) => Ok((ty.clone(), val.into_struct_value())),
            Type::TyVar(_) | Type::App(_, _) => {
                // Try to resolve to flat ADT
                if let Some(adt_ty) = self.types.resolve_to_flat_adt(ty) {
                    Ok((adt_ty, val.into_struct_value()))
                } else {
                    Err(CodeGenError::TypeError(format!(
                        "expected ADT type in match, got {ty:?}"
                    )))
                }
            }
            _ => Err(CodeGenError::TypeError(format!(
                "expected ADT type in match, got {ty:?}"
            ))),
        }
    }

    /// Extract variant info from an ADT type.
    pub(crate) fn get_adt_variants(&self, ty: &Type) -> Result<Vec<(String, Type)>, CodeGenError> {
        match ty {
            Type::Adt(_, _, variants) => Ok(variants.clone()),
            _ => Err(CodeGenError::TypeError(format!(
                "expected Type::Adt, got {ty:?}"
            ))),
        }
    }
}
