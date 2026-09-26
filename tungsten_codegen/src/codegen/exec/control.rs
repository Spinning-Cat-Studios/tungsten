//! Control flow compilation - if/then/else and natrec.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::BasicType;
use inkwell::values::{BasicValue, BasicValueEnum};
use inkwell::AddressSpace;
use inkwell::IntPredicate;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile if-then-else.
    ///
    /// The branch merge routes through the shared planner (ADR 2.7.26b T2):
    /// the source-inferred result type is passed as the planner's authoritative
    /// expected type — in release builds too, not as a debug assertion — a
    /// musttail-terminated branch is excluded from the phi entirely, and a
    /// reachable branch that disagrees with the expected type is a hard error.
    pub(crate) fn compile_if(
        &mut self,
        cond: &Term,
        then_: &Term,
        else_: &Term,
        is_tail: bool,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // cond is NOT in tail position (in_tail_position already false)
        let cond_val = self.compile_term(cond)?.into_int_value();

        // Infer result type BEFORE compiling branches to ensure consistent lowering
        let result_ty = self.infer_term_type(&Term::If(
            Box::new(cond.clone()),
            Box::new(then_.clone()),
            Box::new(else_.clone()),
        ))?;
        let result_llvm_ty = self.types.lower_type(&result_ty);

        let function = self
            .compilation
            .current_fn
            .ok_or_else(|| CodeGenError::LlvmError("no current function".to_string()))?;

        let then_bb = self.context.append_basic_block(function, "then");
        let else_bb = self.context.append_basic_block(function, "else");
        let merge_bb = self.context.append_basic_block(function, "merge");

        self.builder
            .build_conditional_branch(cond_val, then_bb, else_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        let mut then_arm = self.compile_if_branch(then_bb, then_, result_llvm_ty, is_tail)?;
        let mut else_arm = self.compile_if_branch(else_bb, else_, result_llvm_ty, is_tail)?;

        // Both branches share the source-inferred result type — record it so a
        // disagreement self-decodes (ADR 12.7.26c P2).
        then_arm.source_ty = Some(result_ty.clone());
        else_arm.source_ty = Some(result_ty.clone());

        // Plan + build the merge: the source-inferred type is the expected
        // input the reachable branches must agree with (production check).
        let merge_arms = [then_arm, else_arm];
        let site = format!("if/else in `{}`", self.current_fn_name());
        let plan = crate::codegen::exec::merge::plan_merge(
            &merge_arms,
            Some(result_llvm_ty),
            &site,
            Some(&mut self.types),
        )?;

        for arm in &merge_arms {
            self.terminate_merge_arm(arm, merge_bb)?;
        }

        self.build_planned_merge(merge_bb, &plan, result_llvm_ty, "if_result")
    }

    /// Compile one if/else branch: body + cast to the source-inferred result
    /// type (reachable branches only — an unreachable branch's value is the
    /// musttail epilogue's dummy and stays untouched; the planner excludes it).
    fn compile_if_branch(
        &mut self,
        bb: inkwell::basic_block::BasicBlock<'ctx>,
        body: &Term,
        result_llvm_ty: inkwell::types::BasicTypeEnum<'ctx>,
        is_tail: bool,
    ) -> Result<crate::codegen::exec::merge::MergeArm<'ctx>, CodeGenError> {
        self.builder.position_at_end(bb);
        self.compilation.in_tail_position = is_tail;
        let val = self.compile_term(body)?;

        let end_bb = self.builder.get_insert_block().unwrap();
        let reachable = end_bb.get_first_use().is_some();

        // Cast to the consistent (source-inferred) type if needed — only for
        // reachable branches; T1's shrinking-aggregate guard makes a poisoned
        // cast a hard error rather than silent truncation.
        let val = if reachable {
            self.cast_to_type(val, result_llvm_ty)?
        } else {
            val
        };
        let end_bb = self.builder.get_insert_block().unwrap();

        Ok(crate::codegen::exec::merge::MergeArm {
            value: val,
            end_bb,
            reachable,
            // Set by the caller (`compile_if`) from the shared inferred result
            // type once both branches are built (ADR 12.7.26c P2).
            source_ty: None,
        })
    }

    /// Cast a value to a target type, using bitcast through memory if sizes differ.
    ///
    /// Shrinking an *aggregate* (struct/array) source to a smaller destination is
    /// a hard error: no legitimate emission produces that shape — the sole in-tree
    /// producer was the 1.7.26e §6.6 phi-poisoning miscompile, where a merge typed
    /// from a dead musttail arm's `i1` dummy truncated real sret results through a
    /// 1-byte memcpy. Failing here converts that class into a compile-time error
    /// at the emission site (ADR 2.7.26b T1).
    pub(crate) fn cast_to_type(
        &mut self,
        val: BasicValueEnum<'ctx>,
        target_ty: inkwell::types::BasicTypeEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        if val.get_type() == target_ty {
            return Ok(val);
        }

        // Sizes might differ - need to copy through memory
        let src_size = self.type_size_bytes(val.get_type());
        let dst_size = self.type_size_bytes(target_ty);

        // Hard error: shrinking-aggregate cast (ADR 2.7.26b T1). Scalar↔scalar
        // and widening casts keep the memcpy path below (zero-fill unchanged).
        let src_aggregate_kind = match val.get_type() {
            inkwell::types::BasicTypeEnum::StructType(_) => Some("struct"),
            inkwell::types::BasicTypeEnum::ArrayType(_) => Some("array"),
            _ => None,
        };
        if let Some(kind) = src_aggregate_kind {
            if dst_size < src_size {
                let fn_name = self
                    .compilation
                    .current_fn
                    .map(|f| f.get_name().to_string_lossy().into_owned())
                    .unwrap_or_else(|| "<unknown function>".to_string());
                return Err(CodeGenError::TypeError(format!(
                    "shrinking aggregate cast in function `{fn_name}`: refusing to cast {kind} \
                     source {src} ({src_size} bytes) to smaller destination {dst} ({dst_size} \
                     bytes) — this indicates a merge/phi typed from a dead-arm placeholder; \
                     the emitter must exclude unreachable arms from result-type unification \
                     (ADR 2.7.26b T1; see 1.7.26e §6.6)",
                    src = val.get_type().print_to_string(),
                    dst = target_ty.print_to_string(),
                )));
            }
        }

        // Allocate target-sized memory with 16-byte alignment for ARM64
        let alloca = self
            .builder
            .build_alloca(target_ty, "cast_temp")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = alloca.as_instruction() {
            let _ = inst.set_alignment(16);
        }

        // Zero-initialize if target is larger
        if dst_size > src_size {
            let zero = target_ty.const_zero();
            let store = self
                .builder
                .build_store(alloca, zero)
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            let _ = store.set_alignment(16);
        }

        // Store source value (will write to beginning of alloca)
        let src_alloca = self
            .builder
            .build_alloca(val.get_type(), "src_temp")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = src_alloca.as_instruction() {
            let _ = inst.set_alignment(16);
        }
        let store = self
            .builder
            .build_store(src_alloca, val)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(16);

        // Copy bytes
        let copy_size = src_size.min(dst_size);
        let memcpy = self
            .module
            .get_function("memcpy")
            .ok_or_else(|| CodeGenError::LlvmError("memcpy not declared".to_string()))?;

        self.builder
            .build_call(
                memcpy,
                &[
                    alloca.into(),
                    src_alloca.into(),
                    self.context.i64_type().const_int(copy_size, false).into(),
                ],
                "memcpy_cast",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Load as target type with 16-byte alignment
        let result = self
            .builder
            .build_load(target_ty, alloca, "casted")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = result.as_instruction_value() {
            let _ = inst.set_alignment(16);
        }

        Ok(result)
    }

    /// Compile natrec (primitive recursion on naturals).
    pub(crate) fn compile_natrec(
        &mut self,
        result_ty: &Type,
        zero_case: &Term,
        succ_case: &Term,
        n: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let n_val = self.compile_term(n)?.into_int_value();
        let zero_val = self.compile_term(zero_case)?;
        let succ_fn = self.compile_term(succ_case)?;

        let function = self
            .compilation
            .current_fn
            .ok_or_else(|| CodeGenError::LlvmError("no current function".to_string()))?;

        // Create loop structure
        let loop_header = self.context.append_basic_block(function, "natrec_header");
        let loop_body = self.context.append_basic_block(function, "natrec_body");
        let loop_end = self.context.append_basic_block(function, "natrec_end");

        let i64_type = self.context.i64_type();
        let zero = i64_type.const_int(0, false);
        let one = i64_type.const_int(1, false);

        // Initialize counter and accumulator
        let entry_bb = self.builder.get_insert_block().unwrap();
        self.builder
            .build_unconditional_branch(loop_header)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Loop header: check if counter < n
        self.builder.position_at_end(loop_header);
        let counter = self
            .builder
            .build_phi(i64_type, "counter")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let accum = self
            .builder
            .build_phi(zero_val.get_type(), "accum")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        counter.add_incoming(&[(&zero, entry_bb)]);
        accum.add_incoming(&[(&zero_val, entry_bb)]);

        let cmp = self
            .builder
            .build_int_compare(
                IntPredicate::ULT,
                counter.as_basic_value().into_int_value(),
                n_val,
                "cmp",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(cmp, loop_body, loop_end)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Loop body: apply succ_case(counter)(accum)
        self.builder.position_at_end(loop_body);
        let new_accum = self.apply_natrec_succ(result_ty, &succ_fn, &counter, &accum)?;

        let new_counter = self
            .builder
            .build_int_add(counter.as_basic_value().into_int_value(), one, "inc")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        let body_bb = self.builder.get_insert_block().unwrap();
        counter.add_incoming(&[(&new_counter, body_bb)]);
        accum.add_incoming(&[(&new_accum, body_bb)]);

        self.builder
            .build_unconditional_branch(loop_header)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Loop end
        self.builder.position_at_end(loop_end);
        Ok(accum.as_basic_value())
    }

    /// Apply `succ_case(counter)(accum)`: two curried closure calls.
    fn apply_natrec_succ(
        &mut self,
        result_ty: &Type,
        succ_fn: &BasicValueEnum<'ctx>,
        counter: &inkwell::values::PhiValue<'ctx>,
        accum: &inkwell::values::PhiValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let i64_type = self.context.i64_type();
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let partial_ty = self.types.lower_type(result_ty);
        let closure_ty = self
            .context
            .struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);

        let succ_closure = succ_fn.into_struct_value();
        let fn_ptr1 = self
            .builder
            .build_extract_value(succ_closure, 0, "fn_ptr1")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let env_ptr1 = self
            .builder
            .build_extract_value(succ_closure, 1, "env_ptr1")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // First apply to counter
        let fn_type1 = closure_ty.fn_type(&[env_ptr_type.into(), i64_type.into()], false);
        let partial = self
            .builder
            .build_indirect_call(
                fn_type1,
                fn_ptr1,
                &[env_ptr1.into(), counter.as_basic_value().into()],
                "partial",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::TypeError("succ_case returned void".to_string()))?
            .into_struct_value();

        // Then apply to accumulator
        let fn_ptr2 = self
            .builder
            .build_extract_value(partial, 0, "fn_ptr2")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let env_ptr2 = self
            .builder
            .build_extract_value(partial, 1, "env_ptr2")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        let fn_type2 = partial_ty.fn_type(
            &[
                env_ptr_type.into(),
                accum.as_basic_value().get_type().into(),
            ],
            false,
        );

        let new_accum = self
            .builder
            .build_indirect_call(
                fn_type2,
                fn_ptr2,
                &[env_ptr2.into(), accum.as_basic_value().into()],
                "new_accum",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::TypeError("succ_case returned void".to_string()))?;

        // Materialize large struct results to fix ARM64 sret ABI issues
        self.materialize_call_result(new_accum)
    }
}
