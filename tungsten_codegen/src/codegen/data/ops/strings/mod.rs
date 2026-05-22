//! String compilation - literals, concatenation, equality.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::AddressSpace;
use inkwell::IntPredicate;
use tungsten_core::terms::Term;

impl<'ctx> CodeGen<'ctx> {
    /// Compile a string literal to a {ptr, len} struct.
    pub(crate) fn compile_string_lit(
        &mut self,
        s: &str,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let string_struct_type = self
            .context
            .struct_type(&[ptr_type.into(), i64_type.into()], false);

        // Global string constant
        let global = self
            .builder
            .build_global_string_ptr(s, "str_lit")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Create struct
        let string_val = string_struct_type.const_zero();
        let string_val = self
            .builder
            .build_insert_value(string_val, global.as_pointer_value(), 0, "str_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();
        let string_val = self
            .builder
            .build_insert_value(
                string_val,
                i64_type.const_int(s.len() as u64, false),
                1,
                "str_len",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();

        Ok(string_val.into())
    }

    /// Compile string concatenation via FFI call.
    ///
    /// Uses `tg_string_concat_owned` (realloc-based) when the left operand is
    /// a nested `StrConcat` — its result is a heap-allocated temporary that is
    /// dead after this outer concat. Otherwise uses `tg_string_concat` (fresh
    /// allocation) to preserve value semantics.
    pub(crate) fn compile_str_concat(
        &mut self,
        left: &Term,
        right: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let left_val = self.compile_term(left)?.into_struct_value();
        let right_val = self.compile_term(right)?.into_struct_value();

        // Liveness gate: realloc is safe when the left operand is a dead
        // heap-allocated temporary. Two cases qualify:
        // 1. Nested StrConcat — result is an anonymous heap temporary.
        // 2. Let-bound variable that (a) originated from StrConcat (heap_origin)
        //    AND (b) has no further uses after this point (last_use).
        let fn_name = if matches!(left, Term::StrConcat(_, _)) {
            "tg_string_concat_owned"
        } else if let Term::Var(x) = left {
            if self.compilation.last_use_vars.contains(x)
                && self.compilation.heap_origin_vars.contains(x)
            {
                "tg_string_concat_owned"
            } else {
                "tg_string_concat"
            }
        } else {
            "tg_string_concat"
        };

        let concat_fn = self
            .module
            .get_function(fn_name)
            .expect("string concat FFI not declared");

        let result = self
            .builder
            .build_call(
                concat_fn,
                &[left_val.into(), right_val.into()],
                "concat_result",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::LlvmError("tg_string_concat returned void".to_string()))?;

        Ok(result)
    }

    /// Build a memcpy intrinsic call.
    pub(crate) fn build_memcpy(
        &self,
        dest: PointerValue<'ctx>,
        src: PointerValue<'ctx>,
        len: inkwell::values::IntValue<'ctx>,
    ) -> Result<(), CodeGenError> {
        let i1_type = self.context.bool_type();
        let _i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());

        let _memcpy_type = self.context.void_type().fn_type(
            &[
                ptr_type.into(),
                ptr_type.into(),
                i64_type.into(),
                i1_type.into(),
            ],
            false,
        );

        let memcpy_fn = self
            .module
            .get_function("llvm.memcpy.p0.p0.i64")
            .unwrap_or_else(|| {
                // Try to add it with the correct mangled name
                let intrinsic = inkwell::intrinsics::Intrinsic::find("llvm.memcpy")
                    .expect("memcpy intrinsic not found");
                intrinsic
                    .get_declaration(
                        &self.module,
                        &[ptr_type.into(), ptr_type.into(), i64_type.into()],
                    )
                    .expect("couldn't get memcpy declaration")
            });

        let false_val = i1_type.const_int(0, false);
        self.builder
            .build_call(
                memcpy_fn,
                &[dest.into(), src.into(), len.into(), false_val.into()],
                "",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok(())
    }

    /// Compile string equality check.
    pub(crate) fn compile_str_eq(
        &mut self,
        left: &Term,
        right: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let left_val = self.compile_term(left)?.into_struct_value();
        let right_val = self.compile_term(right)?.into_struct_value();

        let function = self
            .compilation
            .current_fn
            .ok_or_else(|| CodeGenError::LlvmError("no current function".to_string()))?;

        let i1_type = self.context.bool_type();
        let _i64_type = self.context.i64_type();

        // Extract lengths and pointers
        let (left_ptr, left_len) = self.extract_string_parts(left_val)?;
        let (right_ptr, right_len) = self.extract_string_parts(right_val)?;

        // Compare lengths first
        let len_eq = self
            .builder
            .build_int_compare(IntPredicate::EQ, left_len, right_len, "len_eq")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Branch structure
        let len_match_bb = self.context.append_basic_block(function, "len_match");
        let result_bb = self.context.append_basic_block(function, "str_eq_result");

        let entry_bb = self.builder.get_insert_block().unwrap();
        self.builder
            .build_conditional_branch(len_eq, len_match_bb, result_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Length matches - compare bytes in a loop
        self.builder.position_at_end(len_match_bb);
        let (loop_eq_bb, loop_neq_bb) =
            self.build_byte_comparison_loop(function, left_ptr, right_ptr, left_len, result_bb)?;

        // Result phi — merge all paths
        self.builder.position_at_end(result_bb);
        let false_val = i1_type.const_int(0, false);
        let true_val = i1_type.const_int(1, false);
        let result_phi = self
            .builder
            .build_phi(i1_type, "str_eq_result")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        result_phi.add_incoming(&[
            (&false_val, entry_bb),    // lengths didn't match
            (&true_val, loop_eq_bb),   // all bytes matched
            (&false_val, loop_neq_bb), // byte mismatch
        ]);

        Ok(result_phi.as_basic_value())
    }

    /// Extract pointer and length from a string struct value.
    fn extract_string_parts(
        &self,
        str_val: inkwell::values::StructValue<'ctx>,
    ) -> Result<
        (
            inkwell::values::PointerValue<'ctx>,
            inkwell::values::IntValue<'ctx>,
        ),
        CodeGenError,
    > {
        let ptr = self
            .builder
            .build_extract_value(str_val, 0, "str_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let len = self
            .builder
            .build_extract_value(str_val, 1, "str_len")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();
        Ok((ptr, len))
    }

    /// Build a byte-by-byte comparison loop over two string buffers.
    ///
    /// Returns the (`loop_eq_bb`, `loop_neq_bb`) blocks that branch to the result phi.
    fn build_byte_comparison_loop(
        &self,
        function: inkwell::values::FunctionValue<'ctx>,
        left_ptr: inkwell::values::PointerValue<'ctx>,
        right_ptr: inkwell::values::PointerValue<'ctx>,
        length: inkwell::values::IntValue<'ctx>,
        result_bb: inkwell::basic_block::BasicBlock<'ctx>,
    ) -> Result<
        (
            inkwell::basic_block::BasicBlock<'ctx>,
            inkwell::basic_block::BasicBlock<'ctx>,
        ),
        CodeGenError,
    > {
        let i8_type = self.context.i8_type();
        let i64_type = self.context.i64_type();
        let _i1_type = self.context.bool_type();

        let loop_header = self.context.append_basic_block(function, "cmp_loop_header");
        let loop_body = self.context.append_basic_block(function, "cmp_loop_body");
        let loop_eq = self.context.append_basic_block(function, "cmp_loop_eq");
        let loop_neq = self.context.append_basic_block(function, "cmp_loop_neq");

        let zero = i64_type.const_int(0, false);
        let one = i64_type.const_int(1, false);
        let len_match_bb = self.builder.get_insert_block().unwrap();
        self.builder
            .build_unconditional_branch(loop_header)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Loop header: check if index < length
        self.builder.position_at_end(loop_header);
        let idx_phi = self
            .builder
            .build_phi(i64_type, "idx")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        idx_phi.add_incoming(&[(&zero, len_match_bb)]);
        let idx = idx_phi.as_basic_value().into_int_value();

        let cmp_done = self
            .builder
            .build_int_compare(IntPredicate::UGE, idx, length, "cmp_done")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(cmp_done, loop_eq, loop_body)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Loop body: compare one byte
        self.builder.position_at_end(loop_body);
        let left_byte_ptr = unsafe {
            self.builder
                .build_gep(i8_type, left_ptr, &[idx], "left_byte_ptr")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
        };
        let right_byte_ptr = unsafe {
            self.builder
                .build_gep(i8_type, right_ptr, &[idx], "right_byte_ptr")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
        };
        let left_byte = self
            .builder
            .build_load(i8_type, left_byte_ptr, "left_byte")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();
        let right_byte = self
            .builder
            .build_load(i8_type, right_byte_ptr, "right_byte")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();
        let bytes_eq = self
            .builder
            .build_int_compare(IntPredicate::EQ, left_byte, right_byte, "bytes_eq")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        let next_idx = self
            .builder
            .build_int_add(idx, one, "next_idx")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let body_bb = self.builder.get_insert_block().unwrap();
        idx_phi.add_incoming(&[(&next_idx, body_bb)]);

        self.builder
            .build_conditional_branch(bytes_eq, loop_header, loop_neq)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Loop eq - all bytes matched
        self.builder.position_at_end(loop_eq);
        self.builder
            .build_unconditional_branch(result_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Loop neq - mismatch found
        self.builder.position_at_end(loop_neq);
        self.builder
            .build_unconditional_branch(result_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok((loop_eq, loop_neq))
    }

    /// Allocate a byte buffer via malloc, returning a pointer.
    pub(crate) fn malloc_bytes(
        &self,
        size: inkwell::values::IntValue<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, CodeGenError> {
        let malloc_fn = self.get_malloc();
        let buf = self
            .builder
            .build_call(malloc_fn, &[size.into()], name)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::LlvmError("malloc returned void".to_string()))?
            .into_pointer_value();
        Ok(buf)
    }

    /// Build a {ptr, len} string struct from a pointer and length.
    pub(crate) fn build_string_struct(
        &self,
        ptr: PointerValue<'ctx>,
        len: inkwell::values::IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let string_struct_type = self
            .context
            .struct_type(&[ptr_type.into(), i64_type.into()], false);
        let result = string_struct_type.const_zero();
        let result = self
            .builder
            .build_insert_value(result, ptr, 0, "res_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();
        let result = self
            .builder
            .build_insert_value(result, len, 1, "res_len")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();
        Ok(result.into())
    }

    /// Helper to compute min(a, b) for unsigned integers.
    pub(crate) fn build_min_nat(
        &self,
        a: inkwell::values::IntValue<'ctx>,
        b: inkwell::values::IntValue<'ctx>,
        name: &str,
    ) -> Result<inkwell::values::IntValue<'ctx>, CodeGenError> {
        use inkwell::IntPredicate;

        let cmp = self
            .builder
            .build_int_compare(IntPredicate::ULT, a, b, "min_cmp")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let result = self
            .builder
            .build_select(cmp, a, b, name)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();
        Ok(result)
    }
}

mod char_ops;

#[cfg(test)]
mod tests;
