//! ADT pattern matching compilation — LLVM `switch`-based dispatch.
//!
//! Generates tag extraction + switch + per-arm payload loading for flat ADT types.

use crate::codegen::backend::CodeGenError;
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
    result_type: Option<inkwell::types::BasicTypeEnum<'ctx>>,
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

        // Compile each arm and collect phi incoming values
        let mut arm_ctx = AdtArmCtx {
            data_ptr,
            variants,
            merge_bb,
            is_tail,
            result_type: None,
        };
        let phi_incoming = self.compile_adt_arms(&switch_info, arms, &mut arm_ctx)?;

        // Build merge block with phi
        self.build_adt_merge_phi(merge_bb, &phi_incoming)
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

    /// Compile all ADT match arms and collect phi incoming values.
    fn compile_adt_arms(
        &mut self,
        arm_blocks: &[(inkwell::basic_block::BasicBlock<'ctx>, String, usize)],
        arms: &[(usize, String, Box<Term>)],
        ctx: &mut AdtArmCtx<'ctx>,
    ) -> Result<Vec<(BasicValueEnum<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)>, CodeGenError>
    {
        let mut phi_incoming = Vec::with_capacity(arms.len());

        for (i, (bb, var_name, variant_idx)) in arm_blocks.iter().enumerate() {
            let (arm_result, end_bb) =
                self.compile_adt_arm(*bb, *variant_idx, var_name, &arms[i].2, ctx)?;
            phi_incoming.push((arm_result, end_bb));
        }

        Ok(phi_incoming)
    }

    /// Compile a single ADT match arm.
    fn compile_adt_arm(
        &mut self,
        bb: inkwell::basic_block::BasicBlock<'ctx>,
        variant_idx: usize,
        var_name: &str,
        body: &Term,
        ctx: &mut AdtArmCtx<'ctx>,
    ) -> Result<(BasicValueEnum<'ctx>, inkwell::basic_block::BasicBlock<'ctx>), CodeGenError> {
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
        if let Some(v) = old_binding {
            self.compilation.env.insert(var_name.to_string(), v);
        } else {
            self.compilation.env.remove(var_name);
        }

        // Track/cast result type for phi consistency
        if ctx.result_type.is_none() {
            ctx.result_type = Some(arm_result.get_type());
        }
        let arm_result = if let Some(expected_ty) = ctx.result_type {
            if arm_result.get_type() == expected_ty {
                arm_result
            } else {
                self.cast_to_type(arm_result, expected_ty)?
            }
        } else {
            arm_result
        };

        // Branch to merge
        let actual_end_bb = self.builder.get_insert_block().unwrap();
        self.builder
            .build_unconditional_branch(ctx.merge_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok((arm_result, actual_end_bb))
    }

    /// Build merge block with phi node for ADT match results.
    fn build_adt_merge_phi(
        &mut self,
        merge_bb: inkwell::basic_block::BasicBlock<'ctx>,
        phi_incoming: &[(BasicValueEnum<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)],
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        self.builder.position_at_end(merge_bb);

        let result_type = phi_incoming
            .first()
            .map(|(val, _)| val.get_type())
            .ok_or_else(|| CodeGenError::TypeError("ADT match has no arms".to_string()))?;

        let phi = self
            .builder
            .build_phi(result_type, "adt_result")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        for (val, bb) in phi_incoming {
            phi.add_incoming(&[(val, *bb)]);
        }

        Ok(phi.as_basic_value())
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
                let inner_ty = self.unwrap_mu_type(ty);

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
