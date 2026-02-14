//! Primitive type compilation.
//!
//! Handles compilation of basic type constructors:
//! - Booleans (True, False)
//! - Unit
//! - Naturals (Zero, Succ, NatLit)
//! - Void (Absurd)

use crate::codegen::error::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::BasicValueEnum;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile `true` literal.
    pub(crate) fn compile_true(&self) -> BasicValueEnum<'ctx> {
        self.context.bool_type().const_int(1, false).into()
    }

    /// Compile `false` literal.
    pub(crate) fn compile_false(&self) -> BasicValueEnum<'ctx> {
        self.context.bool_type().const_int(0, false).into()
    }

    /// Compile unit value `()`.
    pub(crate) fn compile_unit(&self) -> BasicValueEnum<'ctx> {
        let unit_type = self.context.struct_type(&[], false);
        unit_type.const_named_struct(&[]).into()
    }

    /// Compile `zero` (natural number 0).
    pub(crate) fn compile_zero(&self) -> BasicValueEnum<'ctx> {
        self.context.i64_type().const_int(0, false).into()
    }

    /// Compile `succ(n)` (successor of natural number).
    pub(crate) fn compile_succ(
        &self,
        n_val: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let one = self.context.i64_type().const_int(1, false);
        let result = self
            .builder
            .build_int_add(n_val.into_int_value(), one, "succ")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }

    /// Compile natural number literal.
    pub(crate) fn compile_nat_lit(&self, n: u64) -> BasicValueEnum<'ctx> {
        self.context.i64_type().const_int(n, false).into()
    }

    /// Compile `absurd` (elimination of Void type).
    ///
    /// This code is unreachable since Void has no values.
    pub(crate) fn compile_absurd(
        &mut self,
        ty: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let llvm_ty = self.types.lower_type(ty);
        self.builder
            .build_unreachable()
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        // Return undef to satisfy type checker (code will never run)
        Ok(llvm_ty.const_zero())
    }

    /// Compile `sorry` (placeholder for dead code branches).
    ///
    /// Emits unreachable and creates a dead block for subsequent code.
    pub(crate) fn compile_sorry(&mut self) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        self.builder
            .build_unreachable()
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Create a new "dead" basic block for any subsequent code.
        let function = self
            .current_fn
            .ok_or_else(|| CodeGenError::LlvmError("no current function for sorry".to_string()))?;
        let dead_bb = self.context.append_basic_block(function, "dead");
        self.builder.position_at_end(dead_bb);

        // Return a dummy unit value.
        Ok(self.compile_unit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    #[test]
    fn test_compile_true() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test");
        let val = codegen.compile_true();
        assert!(val.is_int_value());
        // bool_type is i1
        assert_eq!(val.into_int_value().get_type().get_bit_width(), 1);
    }

    #[test]
    fn test_compile_false() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test");
        let val = codegen.compile_false();
        assert!(val.is_int_value());
        assert_eq!(val.into_int_value().get_type().get_bit_width(), 1);
    }

    #[test]
    fn test_compile_unit() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test");
        let val = codegen.compile_unit();
        assert!(val.is_struct_value());
        // Unit is an empty struct
        assert_eq!(val.into_struct_value().get_type().count_fields(), 0);
    }

    #[test]
    fn test_compile_zero() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test");
        let val = codegen.compile_zero();
        assert!(val.is_int_value());
        assert_eq!(val.into_int_value().get_type().get_bit_width(), 64);
    }

    #[test]
    fn test_compile_nat_lit() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test");

        let val1 = codegen.compile_nat_lit(0);
        assert!(val1.is_int_value());
        assert_eq!(val1.into_int_value().get_type().get_bit_width(), 64);

        let val2 = codegen.compile_nat_lit(42);
        assert!(val2.is_int_value());
        assert_eq!(val2.into_int_value().get_type().get_bit_width(), 64);

        let val3 = codegen.compile_nat_lit(u64::MAX);
        assert!(val3.is_int_value());
    }
}
