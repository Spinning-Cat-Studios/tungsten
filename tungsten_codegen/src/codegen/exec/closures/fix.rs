//! Fixed point / general recursion compilation.
//!
//! The `fix` combinator enables general recursion by creating a self-referential
//! closure. For a type `A -> B`, it creates a function that can call itself.

use crate::codegen::error::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::{BasicType, StructType};
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue, StructValue};
use inkwell::AddressSpace;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

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
        self.compile_fix_body(fix_fn, f, ty, &param_ty, body, null_env)?;
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
    /// The function has signature `(env_ptr, param) -> ret` where env_ptr
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
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
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
        fix_fn: FunctionValue<'ctx>,
        f: &str,
        fix_ty: &Type,
        param_ty: &Type,
        body: &Term,
        null_env: PointerValue<'ctx>,
    ) -> Result<(), CodeGenError> {
        // Switch to fix function context
        self.current_fn = Some(fix_fn);
        let entry = self.context.append_basic_block(fix_fn, "entry");
        self.builder.position_at_end(entry);
        self.env.clear();

        // Create self-reference closure and add to environment
        let self_closure = self.build_self_reference_closure(fix_fn, null_env)?;
        self.env
            .insert(f.to_string(), (self_closure.into(), fix_ty.clone()));

        // Compile the lambda body
        self.compile_fix_lambda_body(fix_fn, param_ty, body)
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
                self.env.insert(x.clone(), (param_val, param_ty.clone()));

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

        let void_type = context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        let function = codegen.module.add_function("test_fn", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        codegen.builder.position_at_end(entry);
        codegen.current_fn = Some(function);

        codegen
    }

    // ========================================================================
    // Tests for arrow type extraction
    // ========================================================================

    #[test]
    fn test_extract_fix_arrow_type_valid() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let arrow_ty = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool));
        let result = codegen.extract_fix_arrow_type(&arrow_ty);

        assert!(result.is_ok());
        let (param, ret) = result.unwrap();
        assert_eq!(param, Type::Nat);
        assert_eq!(ret, Type::Bool);
    }

    #[test]
    fn test_extract_fix_arrow_type_nested() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        // (Nat -> Bool) -> Nat
        let inner = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool));
        let arrow_ty = Type::Arrow(Box::new(inner.clone()), Box::new(Type::Nat));
        let result = codegen.extract_fix_arrow_type(&arrow_ty);

        assert!(result.is_ok());
        let (param, ret) = result.unwrap();
        assert_eq!(param, inner);
        assert_eq!(ret, Type::Nat);
    }

    #[test]
    fn test_extract_fix_arrow_type_invalid() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let result = codegen.extract_fix_arrow_type(&Type::Nat);
        assert!(result.is_err());

        let result = codegen.extract_fix_arrow_type(&Type::Bool);
        assert!(result.is_err());

        let result = codegen.extract_fix_arrow_type(&Type::Unit);
        assert!(result.is_err());
    }

    // ========================================================================
    // Tests for fix function creation
    // ========================================================================

    #[test]
    fn test_create_fix_function() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        let fix_fn = codegen.create_fix_function(&Type::Nat, &Type::Bool);

        // Verify function exists and has correct arity
        let fn_type = fix_fn.get_type();
        assert_eq!(fn_type.get_param_types().len(), 2); // env_ptr + param
    }

    #[test]
    fn test_create_fix_function_unique_names() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        let fix_fn1 = codegen.create_fix_function(&Type::Nat, &Type::Nat);
        let fix_fn2 = codegen.create_fix_function(&Type::Nat, &Type::Nat);

        // Names should be unique
        assert_ne!(
            fix_fn1.get_name().to_str().unwrap(),
            fix_fn2.get_name().to_str().unwrap()
        );
    }

    // ========================================================================
    // Tests for closure struct type
    // ========================================================================

    #[test]
    fn test_get_closure_struct_type() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let closure_type = codegen.get_closure_struct_type();

        // Should have 2 fields (fn_ptr, env_ptr)
        assert_eq!(closure_type.get_field_types().len(), 2);
    }

    // ========================================================================
    // Tests for self-reference closure building
    // ========================================================================

    #[test]
    fn test_build_self_reference_closure() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create a dummy function
        let env_ptr_type = context.ptr_type(AddressSpace::default());
        let i64_type = context.i64_type();
        let fn_type = i64_type.fn_type(&[env_ptr_type.into(), i64_type.into()], false);
        let fix_fn = codegen.module.add_function("test_fix", fn_type, None);

        let null_env = env_ptr_type.const_null();
        let result = codegen.build_self_reference_closure(fix_fn, null_env);

        assert!(result.is_ok());
        let closure = result.unwrap();
        // Verify it's a struct with 2 fields
        assert_eq!(closure.get_type().get_field_types().len(), 2);
    }

    // ========================================================================
    // Tests for fix closure building
    // ========================================================================

    #[test]
    fn test_build_fix_closure() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create a dummy function
        let env_ptr_type = context.ptr_type(AddressSpace::default());
        let i64_type = context.i64_type();
        let fn_type = i64_type.fn_type(&[env_ptr_type.into(), i64_type.into()], false);
        let fix_fn = codegen.module.add_function("test_fix", fn_type, None);

        let null_env = env_ptr_type.const_null();
        let result = codegen.build_fix_closure(fix_fn, null_env);

        assert!(result.is_ok());
        assert!(result.unwrap().is_struct_value());
    }

    // ========================================================================
    // Tests for fix lambda body validation
    // ========================================================================

    #[test]
    fn test_compile_fix_lambda_body_non_lambda_error() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create a fix function
        let env_ptr_type = context.ptr_type(AddressSpace::default());
        let i64_type = context.i64_type();
        let fn_type = i64_type.fn_type(&[env_ptr_type.into(), i64_type.into()], false);
        let fix_fn = codegen.module.add_function("test_fix", fn_type, None);

        // Set up the function context
        let entry = context.append_basic_block(fix_fn, "entry");
        codegen.builder.position_at_end(entry);
        codegen.current_fn = Some(fix_fn);

        // Try to compile a non-lambda body
        let non_lambda_body = Term::NatLit(42);
        let result = codegen.compile_fix_lambda_body(fix_fn, &Type::Nat, &non_lambda_body);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, CodeGenError::TypeError(_)));
    }
}
