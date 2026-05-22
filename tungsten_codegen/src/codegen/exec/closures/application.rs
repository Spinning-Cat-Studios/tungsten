//! Function application - calling closures.

use super::is_noreturn_function_name;
use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::BasicType;
use inkwell::values::{BasicValue, BasicValueEnum, LLVMTailCallKind, PointerValue, StructValue};
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
    ///
    /// When `is_tail` is true and the callee's function type matches the caller's,
    /// emits `musttail call` + `ret` to guarantee stack frame reuse (TCO).
    pub(crate) fn compile_app(
        &mut self,
        func: &Term,
        arg: &Term,
        is_tail: bool,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // func and arg are NOT in tail position (in_tail_position already false)
        let func_val = self.compile_term(func)?;
        let arg_val = self.compile_term(arg)?;

        // Extract closure from the compiled function value
        let closure = self.extract_closure_from_value(func_val, func)?;

        // Extract function pointer and environment pointer
        let (fn_ptr, env_ptr) = self.extract_closure_components(closure)?;

        // Infer return type and build the call
        let ret_ty =
            self.infer_term_type(&Term::App(Box::new(func.clone()), Box::new(arg.clone())))?;

        // Musttail path: if in tail position and types match, use musttail
        if is_tail {
            if let Some(result) = self.try_emit_musttail(fn_ptr, env_ptr, arg_val, &ret_ty)? {
                return Ok(result);
            }
        }

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
    // Musttail emission
    // ========================================================================

    /// Try to emit a `musttail call` + `ret` for a tail-position closure call.
    ///
    /// Returns `Some(dummy_value)` if musttail was emitted, `None` if the call
    /// is not eligible (type mismatch). When musttail is emitted, the current
    /// block is terminated with `ret` and a dead block is created for any
    /// subsequent code generation.
    ///
    /// LLVM `musttail` guarantees stack frame reuse regardless of optimization
    /// level, preventing stack overflow in tail-recursive functions.
    fn try_emit_musttail(
        &mut self,
        fn_ptr: PointerValue<'ctx>,
        env_ptr: BasicValueEnum<'ctx>,
        arg_val: BasicValueEnum<'ctx>,
        ret_ty: &Type,
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodeGenError> {
        let current_fn = match self.compilation.current_fn {
            Some(f) => f,
            None => return Ok(None),
        };

        let ret_llvm = self.types.lower_type(ret_ty);
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let callee_fn_type =
            ret_llvm.fn_type(&[env_ptr_type.into(), arg_val.get_type().into()], false);

        // musttail requires caller and callee to have identical function types
        if callee_fn_type != current_fn.get_type() {
            return Ok(None);
        }

        // ABI safety: struct returns/params may be incompatible with
        // musttail depending on target and call kind (ADR 12.5.26e/f).
        // Indirect closure calls remain strict on AArch64.
        if let Err(_reason) = self.check_musttail_abi_safety(
            callee_fn_type,
            crate::codegen::abi::MusttailCallKind::IndirectClosure,
        ) {
            return Ok(None);
        }

        // Emit musttail call
        let call_site = self
            .builder
            .build_indirect_call(
                callee_fn_type,
                fn_ptr,
                &[env_ptr.into(), arg_val.into()],
                "musttail_call",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        call_site.set_tail_call_kind(LLVMTailCallKind::LLVMTailCallKindMustTail);

        let result = call_site
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::TypeError("musttail call returned void".to_string()))?;

        // musttail must be immediately followed by ret
        self.builder
            .build_return(Some(&result))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Create dead block for any subsequent code (match phi nodes, etc.)
        if let Some(function) = self.compilation.current_fn {
            let dead_bb = self.context.append_basic_block(function, "musttail_dead");
            self.builder.position_at_end(dead_bb);
        }

        Ok(Some(ret_llvm.const_zero()))
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
                "expected closure (struct or ptr), got {other:?} for func {func_term:?}"
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

        // NOTE: We do NOT mark general closure calls as tail calls.
        // On AArch64, structs > 16 bytes are passed by indirect reference
        // (pointer to caller's stack copy). The `tail` attribute tells LLVM
        // the callee won't access the caller's stack, which conflicts with
        // indirect-reference parameter passing. This caused SIGSEGV in the
        // self-compiled compiler (cursor_peek crash with garbage pointer).
        // True tail calls use musttail (see try_emit_musttail above).

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
        if let Some(function) = self.compilation.current_fn {
            let dead_bb = self.context.append_basic_block(function, "never_dead");
            self.builder.position_at_end(dead_bb);
        }

        // Return dummy value (never executed)
        let ret_llvm = self.types.lower_type(ret_ty);
        Ok(ret_llvm.const_zero())
    }
}

// Tests: application_tests.rs
#[cfg(test)]
#[path = "application_tests.rs"]
mod tests;
