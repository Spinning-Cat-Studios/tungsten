//! Natural number operations compilation.
//!
//! Handles compilation of Nat arithmetic and comparisons:
//! - Arithmetic: `NatAdd`, `NatSub`, `NatMul`, `NatDiv`, `NatMod`
//! - Comparisons: `NatEq`, `NatLt`, `NatLe`, `NatGt`, `NatGe`

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicValueEnum, IntValue};
use inkwell::IntPredicate;

impl<'ctx> CodeGen<'ctx> {
    // ═══════════════════════════════════════════════════════════════════
    // Arithmetic Operations
    // ═══════════════════════════════════════════════════════════════════

    /// Compile `a + b` for natural numbers.
    pub(crate) fn compile_nat_add(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_int_add(a, b, "nat_add")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }

    /// Compile saturating `a - b` for natural numbers.
    ///
    /// Returns `max(a - b, 0)` to handle underflow.
    pub(crate) fn compile_nat_sub(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let diff = self
            .builder
            .build_int_sub(a, b, "nat_sub_raw")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let underflow = self
            .builder
            .build_int_compare(IntPredicate::UGT, b, a, "underflow")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let zero = self.context.i64_type().const_zero();
        let result = self
            .builder
            .build_select(underflow, zero, diff, "nat_sub")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result)
    }

    /// Compile `a * b` for natural numbers.
    pub(crate) fn compile_nat_mul(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_int_mul(a, b, "nat_mul")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }

    /// Compile `a / b` for natural numbers (unsigned division).
    pub(crate) fn compile_nat_div(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_int_unsigned_div(a, b, "nat_div")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }

    /// Compile `a % b` for natural numbers (unsigned remainder).
    pub(crate) fn compile_nat_mod(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_int_unsigned_rem(a, b, "nat_mod")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }

    // ═══════════════════════════════════════════════════════════════════
    // Comparison Operations
    // ═══════════════════════════════════════════════════════════════════

    /// Compile `a == b` for natural numbers.
    pub(crate) fn compile_nat_eq(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_int_compare(IntPredicate::EQ, a, b, "nat_eq")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }

    /// Compile `a < b` for natural numbers.
    pub(crate) fn compile_nat_lt(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_int_compare(IntPredicate::ULT, a, b, "nat_lt")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }

    /// Compile `a <= b` for natural numbers.
    pub(crate) fn compile_nat_le(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_int_compare(IntPredicate::ULE, a, b, "nat_le")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }

    /// Compile `a > b` for natural numbers.
    pub(crate) fn compile_nat_gt(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_int_compare(IntPredicate::UGT, a, b, "nat_gt")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }

    /// Compile `a >= b` for natural numbers.
    pub(crate) fn compile_nat_ge(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_int_compare(IntPredicate::UGE, a, b, "nat_ge")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    fn setup_codegen_with_function(context: &Context) -> CodeGen {
        let mut codegen = CodeGen::new(context, "test");

        // Create a function context for builder operations
        let void_type = context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        let function = codegen.module.add_function("test_fn", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        codegen.builder.position_at_end(entry);
        codegen.compilation.current_fn = Some(function);

        codegen
    }

    #[test]
    fn test_compile_nat_add() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let a = context.i64_type().const_int(5, false);
        let b = context.i64_type().const_int(3, false);

        let result = codegen.compile_nat_add(a, b).unwrap();
        // The result should be an i64 (add instruction result)
        assert!(result.is_int_value());
    }

    #[test]
    fn test_compile_nat_sub_no_underflow() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let a = context.i64_type().const_int(5, false);
        let b = context.i64_type().const_int(3, false);

        let result = codegen.compile_nat_sub(a, b).unwrap();
        // The result should be a select instruction result
        assert!(result.is_int_value());
    }

    #[test]
    fn test_compile_nat_mul() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let a = context.i64_type().const_int(4, false);
        let b = context.i64_type().const_int(3, false);

        let result = codegen.compile_nat_mul(a, b).unwrap();
        assert!(result.is_int_value());
    }

    #[test]
    fn test_compile_nat_div() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let a = context.i64_type().const_int(10, false);
        let b = context.i64_type().const_int(3, false);

        let result = codegen.compile_nat_div(a, b).unwrap();
        assert!(result.is_int_value());
    }

    #[test]
    fn test_compile_nat_mod() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let a = context.i64_type().const_int(10, false);
        let b = context.i64_type().const_int(3, false);

        let result = codegen.compile_nat_mod(a, b).unwrap();
        assert!(result.is_int_value());
    }

    #[test]
    fn test_compile_nat_eq() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let a = context.i64_type().const_int(5, false);
        let b = context.i64_type().const_int(5, false);

        let result = codegen.compile_nat_eq(a, b).unwrap();
        // Comparison returns i1 (bool)
        assert!(result.is_int_value());
        assert_eq!(result.into_int_value().get_type().get_bit_width(), 1);
    }

    #[test]
    fn test_compile_nat_comparisons() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let a = context.i64_type().const_int(3, false);
        let b = context.i64_type().const_int(5, false);

        // All comparisons should return i1
        assert_eq!(
            codegen
                .compile_nat_lt(a, b)
                .unwrap()
                .into_int_value()
                .get_type()
                .get_bit_width(),
            1
        );
        assert_eq!(
            codegen
                .compile_nat_le(a, b)
                .unwrap()
                .into_int_value()
                .get_type()
                .get_bit_width(),
            1
        );
        assert_eq!(
            codegen
                .compile_nat_gt(a, b)
                .unwrap()
                .into_int_value()
                .get_type()
                .get_bit_width(),
            1
        );
        assert_eq!(
            codegen
                .compile_nat_ge(a, b)
                .unwrap()
                .into_int_value()
                .get_type()
                .get_bit_width(),
            1
        );
    }
}
