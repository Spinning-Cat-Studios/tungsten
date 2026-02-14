//! Control flow compilation - if/then/else and natrec.

use crate::codegen::error::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::BasicType;
use inkwell::values::{BasicValue, BasicValueEnum};
use inkwell::AddressSpace;
use inkwell::IntPredicate;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile if-then-else.
    pub(crate) fn compile_if(
        &mut self,
        cond: &Term,
        then_: &Term,
        else_: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let cond_val = self.compile_term(cond)?.into_int_value();

        // Infer result type BEFORE compiling branches to ensure consistent lowering
        let result_ty = self.infer_term_type(&Term::If(
            Box::new(cond.clone()),
            Box::new(then_.clone()),
            Box::new(else_.clone()),
        ))?;
        let result_llvm_ty = self.types.lower_type(&result_ty);

        let function = self
            .current_fn
            .ok_or_else(|| CodeGenError::LlvmError("no current function".to_string()))?;

        let then_bb = self.context.append_basic_block(function, "then");
        let else_bb = self.context.append_basic_block(function, "else");
        let merge_bb = self.context.append_basic_block(function, "merge");

        self.builder
            .build_conditional_branch(cond_val, then_bb, else_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Then branch
        self.builder.position_at_end(then_bb);
        let then_val = self.compile_term(then_)?;
        // Cast to consistent type if needed
        let then_val = self.cast_to_type(then_val, result_llvm_ty)?;
        self.builder
            .build_unconditional_branch(merge_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let then_bb = self.builder.get_insert_block().unwrap();

        // Else branch
        self.builder.position_at_end(else_bb);
        let else_val = self.compile_term(else_)?;
        // Cast to consistent type if needed
        let else_val = self.cast_to_type(else_val, result_llvm_ty)?;
        self.builder
            .build_unconditional_branch(merge_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let else_bb = self.builder.get_insert_block().unwrap();

        // Merge
        self.builder.position_at_end(merge_bb);
        let phi = self
            .builder
            .build_phi(result_llvm_ty, "if_result")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        phi.add_incoming(&[(&then_val, then_bb), (&else_val, else_bb)]);

        Ok(phi.as_basic_value())
    }

    /// Cast a value to a target type, using bitcast through memory if sizes differ.
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

        // Loop body: apply succ_case
        self.builder.position_at_end(loop_body);

        // succ_case : Nat -> T -> T
        // Call: succ_case counter accum
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
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let partial_ty = self.types.lower_type(result_ty);
        let closure_ty = self
            .context
            .struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);
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
        let new_accum = self.materialize_call_result(new_accum)?;

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
}
