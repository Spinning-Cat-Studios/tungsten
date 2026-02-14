//! Function application - calling closures.

use super::is_noreturn_function_name;
use crate::codegen::error::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::BasicType;
use inkwell::values::{BasicValue, BasicValueEnum, PointerValue, StructValue};
use inkwell::AddressSpace;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    // ========================================================================
    // Function application
    // ========================================================================

    /// Compile function application.
    ///
    /// Function application `f x` compiles to:
    /// 1. Extract the closure struct from `f` (handling both direct structs and pointers)
    /// 2. Extract function pointer and environment pointer from the closure
    /// 3. Build an indirect call through the function pointer
    /// 4. Handle noreturn functions by emitting `unreachable`
    pub(crate) fn compile_app(
        &mut self,
        func: &Term,
        arg: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let func_val = self.compile_term(func)?;
        let arg_val = self.compile_term(arg)?;

        // Extract closure from the compiled function value
        let closure = self.extract_closure_from_value(func_val, func)?;

        // Extract function pointer and environment pointer
        let (fn_ptr, env_ptr) = self.extract_closure_components(closure)?;

        // Infer return type and build the call
        let ret_ty =
            self.infer_term_type(&Term::App(Box::new(func.clone()), Box::new(arg.clone())))?;
        let call_result = self.build_closure_call(fn_ptr, env_ptr, arg_val, &ret_ty)?;

        // Handle noreturn functions (Never type or known exit functions)
        let is_noreturn =
            self.types.is_uninhabited_type(&ret_ty) || is_noreturn_function_name(func);
        if is_noreturn {
            return self.emit_noreturn_terminator(&ret_ty);
        }

        // Extract and materialize the call result
        let result = call_result
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::TypeError("function returned void".to_string()))?;

        self.materialize_call_result(result)
    }

    // ========================================================================
    // Application helpers
    // ========================================================================

    /// Extract a closure struct from a compiled function value.
    ///
    /// Handles two cases:
    /// - Direct struct value (from lambda compilation)
    /// - Pointer value (from stored closures)
    fn extract_closure_from_value(
        &mut self,
        func_val: BasicValueEnum<'ctx>,
        func_term: &Term,
    ) -> Result<StructValue<'ctx>, CodeGenError> {
        match func_val {
            BasicValueEnum::StructValue(s) => Ok(s),
            BasicValueEnum::PointerValue(p) => self.load_closure_from_pointer(p),
            other => Err(CodeGenError::TypeError(format!(
                "expected closure (struct or ptr), got {:?} for func {:?}",
                other, func_term
            ))),
        }
    }

    /// Load a closure struct from a pointer.
    fn load_closure_from_pointer(
        &mut self,
        ptr: PointerValue<'ctx>,
    ) -> Result<StructValue<'ctx>, CodeGenError> {
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let closure_type = self
            .context
            .struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);

        let loaded = self
            .builder
            .build_load(closure_type, ptr, "loaded_closure")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Set alignment for ARM64 ABI correctness
        let struct_val = loaded.into_struct_value();
        if let Some(inst) = struct_val.as_instruction_value() {
            let _ = inst.set_alignment(16);
        }

        Ok(struct_val)
    }

    /// Extract function pointer and environment pointer from a closure struct.
    fn extract_closure_components(
        &mut self,
        closure: StructValue<'ctx>,
    ) -> Result<(PointerValue<'ctx>, BasicValueEnum<'ctx>), CodeGenError> {
        let fn_ptr = self
            .builder
            .build_extract_value(closure, 0, "fn_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_pointer_value();

        let env_ptr = self
            .builder
            .build_extract_value(closure, 1, "env_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok((fn_ptr, env_ptr))
    }

    /// Build an indirect call through a closure's function pointer.
    ///
    /// This sets the `tail` attribute on all closure calls as a hint to LLVM.
    /// LLVM will only actually apply TCO if the call is in proper tail position
    /// (i.e., the result flows directly to a return with no intervening operations).
    /// This is critical for recursive functions like `tokenize_loop` to avoid
    /// unbounded stack/heap growth.
    fn build_closure_call(
        &mut self,
        fn_ptr: PointerValue<'ctx>,
        env_ptr: BasicValueEnum<'ctx>,
        arg_val: BasicValueEnum<'ctx>,
        ret_ty: &Type,
    ) -> Result<inkwell::values::CallSiteValue<'ctx>, CodeGenError> {
        let ret_llvm = self.types.lower_type(ret_ty);
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let fn_type = ret_llvm.fn_type(&[env_ptr_type.into(), arg_val.get_type().into()], false);

        let call_site = self
            .builder
            .build_indirect_call(fn_type, fn_ptr, &[env_ptr.into(), arg_val.into()], "call")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Mark as potential tail call - LLVM will only optimize if actually in tail position
        call_site.set_tail_call(true);

        Ok(call_site)
    }

    /// Emit terminator for noreturn function calls.
    ///
    /// Emits LLVM `unreachable` instruction and creates a dead block
    /// for any subsequent code (which will never execute).
    pub(super) fn emit_noreturn_terminator(
        &mut self,
        ret_ty: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        self.builder
            .build_unreachable()
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Create a dead block for any subsequent code
        if let Some(function) = self.current_fn {
            let dead_bb = self.context.append_basic_block(function, "never_dead");
            self.builder.position_at_end(dead_bb);
        }

        // Return dummy value (never executed)
        let ret_llvm = self.types.lower_type(ret_ty);
        Ok(ret_llvm.const_zero())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::CodeGen;
    use inkwell::context::Context;

    /// Create a CodeGen instance with an active function and positioned builder.
    fn setup_codegen_with_function(context: &Context) -> CodeGen<'_> {
        let mut codegen = CodeGen::new(context, "test");

        // Create a simple function to provide a basic block context
        let void_type = context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        let function = codegen.module.add_function("test_fn", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        codegen.builder.position_at_end(entry);
        codegen.current_fn = Some(function);

        codegen
    }

    // ========================================================================
    // Tests for closure extraction (from compile_app helpers)
    // ========================================================================

    #[test]
    fn test_extract_closure_from_struct_value() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create a closure struct value
        let env_ptr_type = context.ptr_type(AddressSpace::default());
        let closure_type = context.struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);
        let closure_val = closure_type.const_zero();

        let result =
            codegen.extract_closure_from_value(closure_val.into(), &Term::Var("f".to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_closure_from_pointer_value() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create a pointer to a closure
        let env_ptr_type = context.ptr_type(AddressSpace::default());
        let closure_type = context.struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);

        // Build alloca and store to create a valid pointer
        let ptr = codegen
            .builder
            .build_alloca(closure_type, "test_closure_ptr")
            .unwrap();
        let zero = closure_type.const_zero();
        codegen.builder.build_store(ptr, zero).unwrap();

        let result = codegen.extract_closure_from_value(ptr.into(), &Term::Var("f".to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_closure_components() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create a closure struct
        let env_ptr_type = context.ptr_type(AddressSpace::default());
        let closure_type = context.struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);
        let closure_val = closure_type.const_zero();

        let result = codegen.extract_closure_components(closure_val);
        assert!(result.is_ok());

        let (_fn_ptr, env_ptr) = result.unwrap();
        // Verify env_ptr is a pointer value
        assert!(env_ptr.is_pointer_value());
    }

    #[test]
    fn test_build_closure_call() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create function pointer and args
        let env_ptr_type = context.ptr_type(AddressSpace::default());
        let i64_type = context.i64_type();

        // Create a dummy function to call
        let fn_type = i64_type.fn_type(&[env_ptr_type.into(), i64_type.into()], false);
        let dummy_fn = codegen.module.add_function("dummy", fn_type, None);

        let fn_ptr = dummy_fn.as_global_value().as_pointer_value();
        let env_ptr = env_ptr_type.const_null();
        let arg_val = i64_type.const_int(42, false);

        let result = codegen.build_closure_call(fn_ptr, env_ptr.into(), arg_val.into(), &Type::Nat);
        assert!(result.is_ok());
    }

    #[test]
    fn test_emit_noreturn_terminator() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        let result = codegen.emit_noreturn_terminator(&Type::Unit);
        assert!(result.is_ok());

        // Verify we're now in a dead block
        let current_block = codegen.builder.get_insert_block();
        assert!(current_block.is_some());
        let block = current_block.unwrap();
        assert!(block.get_name().to_str().unwrap().contains("never_dead"));
    }
}
