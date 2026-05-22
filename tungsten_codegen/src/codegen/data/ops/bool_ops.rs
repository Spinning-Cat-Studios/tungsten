//! Boolean operations compilation.
//!
//! Handles compilation of Bool logic:
//! - `BoolAnd` (&&)
//! - `BoolOr` (||)
//! - `BoolNot` (!)

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicValueEnum, IntValue};

impl<'ctx> CodeGen<'ctx> {
    /// Compile `a && b` for booleans.
    pub(crate) fn compile_bool_and(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_and(a, b, "bool_and")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }

    /// Compile `a || b` for booleans.
    pub(crate) fn compile_bool_or(
        &self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_or(a, b, "bool_or")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(result.into())
    }

    /// Compile `!a` for booleans.
    pub(crate) fn compile_bool_not(
        &self,
        a: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self
            .builder
            .build_not(a, "bool_not")
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
    fn test_compile_bool_and() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let a = context.bool_type().const_int(1, false);
        let b = context.bool_type().const_int(1, false);

        let result = codegen.compile_bool_and(a, b).unwrap();
        assert!(result.is_int_value());
        assert_eq!(result.into_int_value().get_type().get_bit_width(), 1);
    }

    #[test]
    fn test_compile_bool_or() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let a = context.bool_type().const_int(0, false);
        let b = context.bool_type().const_int(1, false);

        let result = codegen.compile_bool_or(a, b).unwrap();
        assert!(result.is_int_value());
        assert_eq!(result.into_int_value().get_type().get_bit_width(), 1);
    }

    #[test]
    fn test_compile_bool_not() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let a = context.bool_type().const_int(1, false);

        let result = codegen.compile_bool_not(a).unwrap();
        assert!(result.is_int_value());
        assert_eq!(result.into_int_value().get_type().get_bit_width(), 1);
    }

    #[test]
    fn test_compile_bool_not_false() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let a = context.bool_type().const_int(0, false);

        let result = codegen.compile_bool_not(a).unwrap();
        assert!(result.is_int_value());
    }

    #[test]
    fn test_bool_and_with_false() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let t = context.bool_type().const_int(1, false);
        let f = context.bool_type().const_int(0, false);

        // true && false
        let result = codegen.compile_bool_and(t, f).unwrap();
        assert!(result.is_int_value());
    }

    #[test]
    fn test_bool_or_with_true() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let t = context.bool_type().const_int(1, false);
        let f = context.bool_type().const_int(0, false);

        // false || true
        let result = codegen.compile_bool_or(f, t).unwrap();
        assert!(result.is_int_value());
    }
}
