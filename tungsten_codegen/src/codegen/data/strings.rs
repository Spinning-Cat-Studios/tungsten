//! String compilation - literals, concatenation, equality.

use crate::codegen::error::CodeGenError;
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

    /// Compile string concatenation.
    pub(crate) fn compile_str_concat(
        &mut self,
        left: &Term,
        right: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let left_val = self.compile_term(left)?.into_struct_value();
        let right_val = self.compile_term(right)?.into_struct_value();

        let i64_type = self.context.i64_type();
        let i8_type = self.context.i8_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let string_struct_type = self
            .context
            .struct_type(&[ptr_type.into(), i64_type.into()], false);

        // Get malloc
        let malloc_fn = self
            .module
            .get_function("malloc")
            .ok_or_else(|| CodeGenError::LlvmError("malloc not found".to_string()))?;

        // Extract pointers and lengths
        let left_ptr = self
            .builder
            .build_extract_value(left_val, 0, "left_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let left_len = self
            .builder
            .build_extract_value(left_val, 1, "left_len")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();
        let right_ptr = self
            .builder
            .build_extract_value(right_val, 0, "right_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let right_len = self
            .builder
            .build_extract_value(right_val, 1, "right_len")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();

        // Total length
        let total_len = self
            .builder
            .build_int_add(left_len, right_len, "total_len")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Allocate buffer
        let buf = self
            .builder
            .build_call(malloc_fn, &[total_len.into()], "concat_buf")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::LlvmError("malloc returned void".to_string()))?
            .into_pointer_value();

        // Copy left string
        self.build_memcpy(buf, left_ptr, left_len)?;

        // Copy right string at offset
        let dest_offset = unsafe {
            self.builder
                .build_gep(i8_type, buf, &[left_len], "dest_offset")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
        };
        self.build_memcpy(dest_offset, right_ptr, right_len)?;

        // Build result struct
        let result = string_struct_type.const_zero();
        let result = self
            .builder
            .build_insert_value(result, buf, 0, "res_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();
        let result = self
            .builder
            .build_insert_value(result, total_len, 1, "res_len")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();

        Ok(result.into())
    }

    /// Build a memcpy intrinsic call.
    pub(crate) fn build_memcpy(
        &self,
        dest: PointerValue<'ctx>,
        src: PointerValue<'ctx>,
        len: inkwell::values::IntValue<'ctx>,
    ) -> Result<(), CodeGenError> {
        let i1_type = self.context.bool_type();
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());

        let memcpy_type = self.context.void_type().fn_type(
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
            .current_fn
            .ok_or_else(|| CodeGenError::LlvmError("no current function".to_string()))?;

        let i1_type = self.context.bool_type();
        let i8_type = self.context.i8_type();
        let i64_type = self.context.i64_type();

        // Extract lengths and pointers
        let left_ptr = self
            .builder
            .build_extract_value(left_val, 0, "left_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let left_len = self
            .builder
            .build_extract_value(left_val, 1, "left_len")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();
        let right_ptr = self
            .builder
            .build_extract_value(right_val, 0, "right_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let right_len = self
            .builder
            .build_extract_value(right_val, 1, "right_len")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();

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

        // Length matches - compare bytes
        self.builder.position_at_end(len_match_bb);

        // Use memcmp or manual loop
        // For simplicity, use a loop
        let loop_header = self.context.append_basic_block(function, "cmp_loop_header");
        let loop_body = self.context.append_basic_block(function, "cmp_loop_body");
        let loop_eq = self.context.append_basic_block(function, "cmp_loop_eq");
        let loop_neq = self.context.append_basic_block(function, "cmp_loop_neq");

        let zero = i64_type.const_int(0, false);
        let one = i64_type.const_int(1, false);
        self.builder
            .build_unconditional_branch(loop_header)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        self.builder.position_at_end(loop_header);
        let idx_phi = self
            .builder
            .build_phi(i64_type, "idx")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        idx_phi.add_incoming(&[(&zero, len_match_bb)]);
        let idx = idx_phi.as_basic_value().into_int_value();

        let cmp_done = self
            .builder
            .build_int_compare(IntPredicate::UGE, idx, left_len, "cmp_done")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder
            .build_conditional_branch(cmp_done, loop_eq, loop_body)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Loop body - compare one byte
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
        let true_val = i1_type.const_int(1, false);
        self.builder
            .build_unconditional_branch(result_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Loop neq - mismatch found
        self.builder.position_at_end(loop_neq);
        let false_val = i1_type.const_int(0, false);
        self.builder
            .build_unconditional_branch(result_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Result phi
        self.builder.position_at_end(result_bb);
        let result_phi = self
            .builder
            .build_phi(i1_type, "str_eq_result")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        result_phi.add_incoming(&[
            (&false_val, entry_bb), // lengths didn't match
            (&true_val, loop_eq),   // all bytes matched
            (&false_val, loop_neq), // byte mismatch
        ]);

        Ok(result_phi.as_basic_value())
    }

    /// Compile char_at(s, n) - get ASCII code of character at index.
    pub(crate) fn compile_str_char_at(
        &mut self,
        s: &Term,
        idx: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let str_val = self.compile_term(s)?.into_struct_value();
        let idx_val = self.compile_term(idx)?.into_int_value();

        let i8_type = self.context.i8_type();
        let i64_type = self.context.i64_type();

        // Extract string pointer
        let str_ptr = self
            .builder
            .build_extract_value(str_val, 0, "str_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_pointer_value();

        // GEP to the index
        let char_ptr = unsafe {
            self.builder
                .build_gep(i8_type, str_ptr, &[idx_val], "char_ptr")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
        };

        // Load the byte
        let char_byte = self
            .builder
            .build_load(i8_type, char_ptr, "char_byte")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();

        // Zero-extend to i64 (Nat)
        let char_nat = self
            .builder
            .build_int_z_extend(char_byte, i64_type, "char_nat")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok(char_nat.into())
    }

    /// Compile substring(s, start, len) - extract a substring.
    pub(crate) fn compile_str_substring(
        &mut self,
        s: &Term,
        start: &Term,
        len: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let str_val = self.compile_term(s)?.into_struct_value();
        let start_val = self.compile_term(start)?.into_int_value();
        let len_val = self.compile_term(len)?.into_int_value();

        let i8_type = self.context.i8_type();
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let string_struct_type = self
            .context
            .struct_type(&[ptr_type.into(), i64_type.into()], false);

        // Extract original string pointer and length
        let str_ptr = self
            .builder
            .build_extract_value(str_val, 0, "str_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let str_len = self
            .builder
            .build_extract_value(str_val, 1, "str_len")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();

        // Clamp start to string length
        let start_clamped = self.build_min_nat(start_val, str_len, "start_clamped")?;

        // Calculate remaining length from start
        let remaining = self
            .builder
            .build_int_sub(str_len, start_clamped, "remaining")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Clamp requested length to remaining
        let actual_len = self.build_min_nat(len_val, remaining, "actual_len")?;

        // Get malloc
        let malloc_fn = self
            .module
            .get_function("malloc")
            .ok_or_else(|| CodeGenError::LlvmError("malloc not found".to_string()))?;

        // Allocate buffer for substring
        let buf = self
            .builder
            .build_call(malloc_fn, &[actual_len.into()], "substr_buf")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::LlvmError("malloc returned void".to_string()))?
            .into_pointer_value();

        // Calculate source pointer (str_ptr + start)
        let src_ptr = unsafe {
            self.builder
                .build_gep(i8_type, str_ptr, &[start_clamped], "src_ptr")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
        };

        // Copy the substring bytes
        self.build_memcpy(buf, src_ptr, actual_len)?;

        // Build result string struct
        let result = string_struct_type.const_zero();
        let result = self
            .builder
            .build_insert_value(result, buf, 0, "substr_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();
        let result = self
            .builder
            .build_insert_value(result, actual_len, 1, "substr_len")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();

        Ok(result.into())
    }

    /// Helper to compute min(a, b) for unsigned integers.
    fn build_min_nat(
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
