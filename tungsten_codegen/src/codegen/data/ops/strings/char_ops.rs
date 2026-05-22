//! String character operations — `char_at`, substring, and helpers.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::BasicValueEnum;
use tungsten_core::terms::Term;

impl<'ctx> CodeGen<'ctx> {
    /// Compile `char_at(s`, n) - get ASCII code of character at index.
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

        // Extract original string pointer and length
        let (str_ptr, str_len) = self.extract_string_parts(str_val)?;

        // Clamp start to string length
        let start_clamped = self.build_min_nat(start_val, str_len, "start_clamped")?;

        // Calculate remaining length from start
        let remaining = self
            .builder
            .build_int_sub(str_len, start_clamped, "remaining")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Clamp requested length to remaining
        let actual_len = self.build_min_nat(len_val, remaining, "actual_len")?;

        // Allocate buffer for substring
        let buf = self.malloc_bytes(actual_len, "substr_buf")?;

        // Calculate source pointer (str_ptr + start)
        let src_ptr = unsafe {
            self.builder
                .build_gep(i8_type, str_ptr, &[start_clamped], "src_ptr")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
        };

        // Copy the substring bytes
        self.build_memcpy(buf, src_ptr, actual_len)?;

        self.build_string_struct(buf, actual_len)
    }
}
