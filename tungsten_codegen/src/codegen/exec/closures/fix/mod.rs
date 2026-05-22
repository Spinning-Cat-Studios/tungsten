//! Fixed point / general recursion compilation.
//!
//! The `fix` combinator enables general recursion by creating a self-referential
//! closure. For a type `A -> B`, it creates a function that can call itself.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::{BasicType, StructType};
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue, StructValue};
use inkwell::AddressSpace;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

/// Context for compiling a fix function body.
struct FixBodyCtx<'a, 'ctx> {
    fix_fn: FunctionValue<'ctx>,
    name: &'a str,
    fix_ty: &'a Type,
    null_env: PointerValue<'ctx>,
}

impl<'ctx> CodeGen<'ctx> {
    // ========================================================================
    // Fixed point compilation - main entry point
    // ========================================================================

    /// Compile fixed point (general recursion).
    ///
    /// `fix f : A -> B = body` creates a recursive function where `f` refers
    /// to itself within `body`. The body must be a lambda expression.
    ///
    /// # Implementation
    ///
    /// 1. Create an LLVM function with signature `(env_ptr, param) -> ret`
    /// 2. Inside the function, create a self-referential closure for `f`
    /// 3. Compile the lambda body with `f` bound to the self-reference
    /// 4. Return a closure wrapping the generated function
    pub(crate) fn compile_fix(
        &mut self,
        f: &str,
        ty: &Type,
        body: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Extract arrow type components
        let (param_ty, ret_ty) = self.extract_fix_arrow_type(ty)?;

        // Create the recursive function
        let fix_fn = self.create_fix_function(&param_ty, &ret_ty);
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let null_env = env_ptr_type.const_null();

        // Save state and compile the function body
        let saved_state = self.save_lambda_state();
        let fix_ctx = FixBodyCtx {
            fix_fn,
            name: f,
            fix_ty: ty,
            null_env,
        };
        self.compile_fix_body(&fix_ctx, &param_ty, body)?;
        self.restore_lambda_state(saved_state);

        // Return closure pointing to the fix function
        self.build_fix_closure(fix_fn, null_env)
    }

    // ========================================================================
    // Fixed point compilation - helper methods
    // ========================================================================

    /// Extract parameter and return types from an arrow type.
    ///
    /// Returns an error if the type is not an arrow type.
    fn extract_fix_arrow_type(&self, ty: &Type) -> Result<(Type, Type), CodeGenError> {
        match ty {
            Type::Arrow(param_ty, ret_ty) => {
                Ok((param_ty.as_ref().clone(), ret_ty.as_ref().clone()))
            }
            _ => Err(CodeGenError::TypeError(
                "fix must have function type".to_string(),
            )),
        }
    }

    /// Create an LLVM function for the fix combinator.
    ///
    /// The function has signature `(env_ptr, param) -> ret` where `env_ptr`
    /// is unused (null) since fix doesn't capture external variables.
    fn create_fix_function(&mut self, param_ty: &Type, ret_ty: &Type) -> FunctionValue<'ctx> {
        let fn_name = self.fresh_name("fix");
        let param_llvm = self.types.lower_type(param_ty);
        let ret_llvm = self.types.lower_type(ret_ty);
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());

        let fn_type = ret_llvm.fn_type(&[env_ptr_type.into(), param_llvm.into()], false);
        self.module.add_function(&fn_name, fn_type, None)
    }

    /// Build a self-referential closure for use inside the fix function.
    ///
    /// This closure allows the recursive function to call itself.
    fn build_self_reference_closure(
        &mut self,
        fix_fn: FunctionValue<'ctx>,
        null_env: PointerValue<'ctx>,
    ) -> Result<StructValue<'ctx>, CodeGenError> {
        let _env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let closure_type = self.get_closure_struct_type();

        let mut self_closure = closure_type.const_zero();
        self_closure = self
            .builder
            .build_insert_value(
                self_closure,
                fix_fn.as_global_value().as_pointer_value(),
                0,
                "self_fn",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();

        self_closure = self
            .builder
            .build_insert_value(self_closure, null_env, 1, "self_env")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();

        Ok(self_closure)
    }

    /// Get the standard closure struct type `{ fn_ptr, env_ptr }`.
    fn get_closure_struct_type(&self) -> StructType<'ctx> {
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        self.context
            .struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false)
    }

    /// Compile the body of a fix function.
    ///
    /// Sets up the function context, creates the self-reference, and compiles
    /// the lambda body.
    fn compile_fix_body(
        &mut self,
        ctx: &FixBodyCtx<'_, 'ctx>,
        param_ty: &Type,
        body: &Term,
    ) -> Result<(), CodeGenError> {
        // Switch to fix function context
        self.compilation.current_fn = Some(ctx.fix_fn);
        let entry = self.context.append_basic_block(ctx.fix_fn, "entry");
        self.builder.position_at_end(entry);
        self.compilation.env.clear();

        // Create self-reference closure and add to environment
        let self_closure = self.build_self_reference_closure(ctx.fix_fn, ctx.null_env)?;
        self.compilation.env.insert(
            ctx.name.to_string(),
            (self_closure.into(), ctx.fix_ty.clone()),
        );

        // Compile the lambda body
        self.compile_fix_lambda_body(ctx.fix_fn, param_ty, body)
    }

    /// Compile the lambda that forms the body of a fix expression.
    ///
    /// The body must be a lambda expression. This extracts the parameter,
    /// compiles the inner body, and emits a return.
    fn compile_fix_lambda_body(
        &mut self,
        fix_fn: FunctionValue<'ctx>,
        param_ty: &Type,
        body: &Term,
    ) -> Result<(), CodeGenError> {
        match body {
            Term::Lambda(x, _, inner_body) => {
                // Get the parameter value from the function
                let param_val = fix_fn.get_nth_param(1).ok_or_else(|| {
                    CodeGenError::TypeError("fix function missing parameter".to_string())
                })?;

                // Add parameter to environment
                self.compilation
                    .env
                    .insert(x.clone(), (param_val, param_ty.clone()));

                // Body is in tail position of this function
                self.compilation.in_tail_position = true;

                // Compile body and return
                let result = self.compile_term(inner_body)?;
                self.builder
                    .build_return(Some(&result))
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                Ok(())
            }
            _ => Err(CodeGenError::TypeError(
                "fix body must be a lambda".to_string(),
            )),
        }
    }

    /// Build the final closure struct for a fix function.
    fn build_fix_closure(
        &mut self,
        fix_fn: FunctionValue<'ctx>,
        null_env: PointerValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let closure_type = self.get_closure_struct_type();

        let mut closure = closure_type.const_zero();
        closure = self
            .builder
            .build_insert_value(
                closure,
                fix_fn.as_global_value().as_pointer_value(),
                0,
                "fix_fn",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();

        closure = self
            .builder
            .build_insert_value(closure, null_env, 1, "fix_env")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();

        Ok(closure.into())
    }
}

#[cfg(test)]
mod tests;
