//! Reference operations compilation.
//!
//! Handles compilation of mutable references:
//! - RefNew: allocate a new reference
//! - RefGet: dereference (read)
//! - RefSet: assign to a reference (write)
//!
//! NOTE: RefNew allocates via malloc but never frees. This is a known leak.
//! Proper memory management (GC or borrow checker) is planned for v2.0.

use crate::codegen::error::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValue, BasicValueEnum, PointerValue};

impl<'ctx> CodeGen<'ctx> {
    /// Compile `ref.new(value)` - allocate a new reference.
    ///
    /// Allocates heap memory via malloc and stores the initial value.
    pub(crate) fn compile_ref_new(
        &self,
        val_compiled: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let val_ty = val_compiled.get_type();

        // Get malloc (declared in declare_runtime_functions)
        let malloc = self
            .module
            .get_function("malloc")
            .ok_or_else(|| CodeGenError::LlvmError("malloc not declared".to_string()))?;

        // Allocate heap memory for the ref
        let size = self.type_size_bytes(val_ty);
        let size_val = self.context.i64_type().const_int(size, false);

        let ptr = self
            .builder
            .build_call(malloc, &[size_val.into()], "ref_alloc")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::LlvmError("malloc returned void".to_string()))?;

        // Store initial value with 16-byte alignment for ARM64 ABI
        let store = self
            .builder
            .build_store(ptr.into_pointer_value(), val_compiled)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(16);

        Ok(ptr)
    }

    /// Compile `ref.get(ref)` - dereference a reference.
    pub(crate) fn compile_ref_get(
        &self,
        ref_ptr: PointerValue<'ctx>,
        inner_ty: BasicTypeEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let loaded = self
            .builder
            .build_load(inner_ty, ref_ptr, "ref_get")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        // Set 16-byte alignment on load for ARM64 ABI
        if let Some(inst) = loaded.as_instruction_value() {
            let _ = inst.set_alignment(16);
        }
        Ok(loaded)
    }

    /// Compile `ref.set(ref, value)` - assign to a reference.
    ///
    /// Returns unit `()`.
    pub(crate) fn compile_ref_set(
        &self,
        ref_ptr: PointerValue<'ctx>,
        val_compiled: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let store = self
            .builder
            .build_store(ref_ptr, val_compiled)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        // Set 16-byte alignment on store for ARM64 ABI
        let _ = store.set_alignment(16);

        // Return unit
        let unit_type = self.context.struct_type(&[], false);
        Ok(unit_type.const_named_struct(&[]).into())
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
        codegen.current_fn = Some(function);

        codegen
    }

    #[test]
    fn test_compile_ref_new() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let val = context.i64_type().const_int(42, false);
        let result = codegen.compile_ref_new(val.into()).unwrap();

        // RefNew returns a pointer
        assert!(result.is_pointer_value());
    }

    #[test]
    fn test_compile_ref_set_returns_unit() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        // Allocate a ref first
        let val = context.i64_type().const_int(42, false);
        let ptr = codegen.compile_ref_new(val.into()).unwrap();

        // Set a new value
        let new_val = context.i64_type().const_int(99, false);
        let result = codegen
            .compile_ref_set(ptr.into_pointer_value(), new_val.into())
            .unwrap();

        // RefSet returns unit (empty struct)
        assert!(result.is_struct_value());
        assert_eq!(result.into_struct_value().get_type().count_fields(), 0);
    }

    #[test]
    fn test_compile_ref_get() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        // Allocate a ref first
        let val = context.i64_type().const_int(42, false);
        let ptr = codegen.compile_ref_new(val.into()).unwrap();

        // Get the value
        let inner_ty = context.i64_type().into();
        let result = codegen
            .compile_ref_get(ptr.into_pointer_value(), inner_ty)
            .unwrap();

        // RefGet returns the inner type (i64 in this case)
        assert!(result.is_int_value());
        assert_eq!(result.into_int_value().get_type().get_bit_width(), 64);
    }

    #[test]
    fn test_ref_new_with_struct() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        // Create a struct value
        let struct_type = context.struct_type(
            &[context.i64_type().into(), context.i64_type().into()],
            false,
        );
        let struct_val = struct_type.const_zero();

        let result = codegen.compile_ref_new(struct_val.into()).unwrap();
        assert!(result.is_pointer_value());
    }
}
