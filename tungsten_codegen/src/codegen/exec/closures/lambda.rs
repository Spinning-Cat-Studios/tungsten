//! Lambda compilation - converting lambda expressions to closures.
//!
//! # Type Sourcing (`compile_lambda`)
//!
//! MIXED: return type comes from the term's type annotation (or `expected_lambda_ret_type`
//! override from Phase 0 mitigation), body type is inferred. Mismatch between annotation
//! and body → Phase 0 mitigation #3 (body cast).

use super::{collect_free_variables, CaptureInfo, SavedLambdaState};
use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::AddressSpace;
use std::collections::HashMap;
use tungsten_core::types::Type;

/// Capture context for compiling a lambda body: the captured variables
/// and the outer environment they were captured from.
struct CaptureCtx<'a, 'ctx> {
    info: &'a CaptureInfo<'ctx>,
    outer_env: &'a HashMap<String, (BasicValueEnum<'ctx>, Type)>,
}
use tungsten_core::terms::Term;

impl<'ctx> CodeGen<'ctx> {
    // ========================================================================
    // Lambda compilation - main entry point
    // ========================================================================

    /// Compile a lambda expression to a closure.
    ///
    /// A closure is a pair `{ fn_ptr, env_ptr }` where:
    /// - `fn_ptr` points to a generated function taking `(env*, param) -> ret`
    /// - `env_ptr` points to a heap-allocated struct containing captured variables
    ///   (or null if no variables are captured)
    pub(crate) fn compile_lambda(
        &mut self,
        param_name: &str,
        param_ty: &Type,
        body: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let resolved_param_ty = self.types.apply_type_subst(param_ty);

        // Phase 1: Analyze captures and infer types
        let free_vars = collect_free_variables(body, param_name);
        let capture_info = self.build_capture_info(&free_vars);

        // Use expected return type from enclosing function when available.
        // This fixes type mismatches where TyVar-corrupted annotations produce
        // wrong-sized sum types (e.g., Sum(Unit, TyVar("T")) → 8 bytes instead
        // of Sum(Unit, ConcreteType) → N bytes).
        let ret_ty = if let Some(expected) = self.compilation.expected_lambda_ret_type.take() {
            // If the expected type itself is Arrow, propagate the inner return
            // for further nested lambdas
            if let Type::Arrow(_, inner_ret) = &expected {
                self.compilation.expected_lambda_ret_type = Some(inner_ret.as_ref().clone());
            }
            expected
        } else {
            self.infer_lambda_return_type(param_name, &resolved_param_ty, body)?
        };

        // Phase 2: Create the lambda function
        let lambda_fn = self.create_lambda_function(&resolved_param_ty, &ret_ty);

        // Phase 3: Compile the lambda body (switches to new function context)
        let saved_state = self.save_lambda_state();
        let capture_ctx = CaptureCtx {
            info: &capture_info,
            outer_env: &saved_state.env,
        };
        self.compile_lambda_body(
            lambda_fn,
            param_name,
            &resolved_param_ty,
            body,
            &capture_ctx,
        )?;
        self.restore_lambda_state(saved_state);

        // Phase 4: Allocate environment and build closure struct
        let env_ptr = self.allocate_lambda_environment(&capture_info)?;
        self.build_closure_struct(lambda_fn, env_ptr)
    }

    // ========================================================================
    // Lambda compilation - helper methods
    // ========================================================================

    /// Build information about captured variables for a lambda.
    fn build_capture_info(&self, free_vars: &[String]) -> CaptureInfo<'ctx> {
        let field_types: Vec<BasicTypeEnum<'ctx>> = free_vars
            .iter()
            .filter_map(|v| self.compilation.env.get(v).map(|(val, _)| val.get_type()))
            .collect();

        let env_struct_type = self.context.struct_type(&field_types, false);

        CaptureInfo {
            names: free_vars.to_vec(),
            field_types,
            env_struct_type,
        }
    }

    /// Infer the return type of a lambda body.
    ///
    /// Temporarily adds the parameter to the environment for type inference,
    /// then restores the original environment state.
    fn infer_lambda_return_type(
        &mut self,
        param_name: &str,
        param_ty: &Type,
        body: &Term,
    ) -> Result<Type, CodeGenError> {
        let placeholder = self.types.lower_type(param_ty).const_zero();
        let old_entry = self
            .compilation
            .env
            .insert(param_name.to_string(), (placeholder, param_ty.clone()));

        let ret_ty = self.infer_term_type(body)?;

        // Restore environment
        if let Some(old) = old_entry {
            self.compilation.env.insert(param_name.to_string(), old);
        } else {
            self.compilation.env.remove(param_name);
        }

        Ok(ret_ty)
    }

    /// Create an LLVM function for a lambda.
    ///
    /// The function signature is `(env_ptr, param) -> ret`.
    fn create_lambda_function(&mut self, param_ty: &Type, ret_ty: &Type) -> FunctionValue<'ctx> {
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let param_llvm = self.types.lower_type(param_ty);
        let ret_llvm = self.types.lower_type(ret_ty);

        let fn_type = ret_llvm.fn_type(&[env_ptr_type.into(), param_llvm.into()], false);
        let lambda_name = self.fresh_lambda_name();
        self.module.add_function(&lambda_name, fn_type, None)
    }

    /// Save the current codegen state before compiling a lambda body.
    pub(super) fn save_lambda_state(&self) -> SavedLambdaState<'ctx> {
        SavedLambdaState {
            current_fn: self.compilation.current_fn,
            env: self.compilation.env.clone(),
            insert_block: self.builder.get_insert_block(),
            in_tail_position: self.compilation.in_tail_position,
        }
    }

    /// Restore codegen state after compiling a lambda body.
    pub(super) fn restore_lambda_state(&mut self, state: SavedLambdaState<'ctx>) {
        self.compilation.current_fn = state.current_fn;
        self.compilation.env = state.env;
        self.compilation.in_tail_position = state.in_tail_position;
        if let Some(block) = state.insert_block {
            self.builder.position_at_end(block);
        }
    }

    /// Compile the body of a lambda function.
    ///
    /// This switches to the lambda's function context, loads captured variables,
    /// compiles the body, and emits a return instruction.
    fn compile_lambda_body(
        &mut self,
        lambda_fn: FunctionValue<'ctx>,
        param_name: &str,
        param_ty: &Type,
        body: &Term,
        capture: &CaptureCtx<'_, 'ctx>,
    ) -> Result<(), CodeGenError> {
        // Switch to lambda function context
        self.compilation.current_fn = Some(lambda_fn);
        let entry = self.context.append_basic_block(lambda_fn, "entry");
        self.builder.position_at_end(entry);
        self.compilation.env.clear();

        // Load captured variables from environment struct
        let env_param = lambda_fn.get_first_param().unwrap().into_pointer_value();
        self.load_captured_variables(env_param, capture.info, capture.outer_env)?;

        // Add parameter to environment
        let param_val = lambda_fn
            .get_nth_param(1)
            .ok_or_else(|| CodeGenError::TypeError("lambda missing parameter".to_string()))?;
        self.compilation
            .env
            .insert(param_name.to_string(), (param_val, param_ty.clone()));

        // Body is in tail position of this function
        self.compilation.in_tail_position = true;

        // Compile body and return, casting to match the function's declared return type
        let body_val = self.compile_term(body)?;
        let body_val = if let Some(expected_ret_ty) = lambda_fn.get_type().get_return_type() {
            self.cast_to_type(body_val, expected_ret_ty)?
        } else {
            body_val
        };
        self.builder
            .build_return(Some(&body_val))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok(())
    }

    /// Load captured variables from an environment struct into the local environment.
    fn load_captured_variables(
        &mut self,
        env_ptr: PointerValue<'ctx>,
        capture_info: &CaptureInfo<'ctx>,
        outer_env: &HashMap<String, (BasicValueEnum<'ctx>, Type)>,
    ) -> Result<(), CodeGenError> {
        for (i, var_name) in capture_info.names.iter().enumerate() {
            if let Some((_, ty)) = outer_env.get(var_name) {
                let field_ptr = self
                    .builder
                    .build_struct_gep(
                        capture_info.env_struct_type,
                        env_ptr,
                        i as u32,
                        &format!("{var_name}_ptr"),
                    )
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                let field_val = self
                    .builder
                    .build_load(capture_info.field_types[i], field_ptr, var_name)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                // Set alignment for ARM64 ABI correctness
                if let Some(inst) = field_val.as_instruction_value() {
                    let _ = inst.set_alignment(16);
                }

                self.compilation
                    .env
                    .insert(var_name.clone(), (field_val, ty.clone()));
            }
        }
        Ok(())
    }

    /// Allocate heap memory for lambda environment and store captured values.
    ///
    /// Returns null pointer if there are no captures (optimization to avoid malloc).
    fn allocate_lambda_environment(
        &mut self,
        capture_info: &CaptureInfo<'ctx>,
    ) -> Result<PointerValue<'ctx>, CodeGenError> {
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());

        if capture_info.names.is_empty() {
            // No captures - use null pointer to avoid malloc overhead
            return Ok(env_ptr_type.const_null());
        }

        // Calculate environment size using the struct type's total size,
        // which correctly accounts for alignment padding between fields.
        let env_size = self.context.i64_type().const_int(
            self.type_size_bytes(capture_info.env_struct_type.into()),
            false,
        );

        // Allocate memory (uses profiling wrapper when --alloc-profile is enabled)
        let malloc = self.get_malloc();

        let env_ptr = self
            .builder
            .build_call(malloc, &[env_size.into()], "env_alloc")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::LlvmError("malloc returned void".to_string()))?
            .into_pointer_value();

        // Store captured variables
        self.store_captured_variables(env_ptr, capture_info)?;

        Ok(env_ptr)
    }

    /// Store captured variable values into an allocated environment struct.
    fn store_captured_variables(
        &mut self,
        env_ptr: PointerValue<'ctx>,
        capture_info: &CaptureInfo<'ctx>,
    ) -> Result<(), CodeGenError> {
        for (i, var_name) in capture_info.names.iter().enumerate() {
            if let Some((val, _)) = self.compilation.env.get(var_name) {
                let field_ptr = self
                    .builder
                    .build_struct_gep(
                        capture_info.env_struct_type,
                        env_ptr,
                        i as u32,
                        &format!("{var_name}_store"),
                    )
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                let store = self
                    .builder
                    .build_store(field_ptr, *val)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                // Set alignment for ARM64 ABI correctness
                let _ = store.set_alignment(16);
            }
        }
        Ok(())
    }

    /// Build the final closure struct `{ fn_ptr, env_ptr }`.
    pub(super) fn build_closure_struct(
        &mut self,
        lambda_fn: FunctionValue<'ctx>,
        env_ptr: PointerValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let closure_type = self
            .context
            .struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);

        let mut closure = closure_type.const_zero();
        closure = self
            .builder
            .build_insert_value(
                closure,
                lambda_fn.as_global_value().as_pointer_value(),
                0,
                "closure_fn",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();

        closure = self
            .builder
            .build_insert_value(closure, env_ptr, 1, "closure_env")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();

        Ok(closure.into())
    }
}

// Tests: lambda_tests.rs
#[cfg(test)]
#[path = "lambda_tests.rs"]
mod tests;
