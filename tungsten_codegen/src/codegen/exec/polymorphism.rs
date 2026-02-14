//! Polymorphism compilation (monomorphization).
//!
//! Handles compilation of:
//! - TyAbs (type abstraction / forall intro)
//! - TyApp (type application / forall elim)
//! - Monomorphized function specialization

use crate::codegen::error::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile type application `t[ty_arg]`.
    ///
    /// This handles:
    /// - Direct specialization: `(TyAbs var body)[ty_arg]` → substitute and compile
    /// - Global specialization: `global_fn[ty_arg]` → monomorphize if needed
    /// - Fallback: just compile the inner term (type erasure)
    pub(crate) fn compile_ty_app(
        &mut self,
        t: &Term,
        ty_arg: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        match t {
            Term::TyAbs(var, body) => {
                // Direct specialization
                self.types.push_type_subst(var.clone(), ty_arg.clone());
                let result = self.compile_term(body);
                self.types.clear_type_subst(); // Simple approach: clear all
                result
            }
            Term::Global(name) => {
                // Apply current type substitution to the type argument
                // This handles cases like calling list_reverse_acc<T> inside list_reverse<String>
                // where T should be substituted with String
                let resolved_ty_arg = self.types.apply_type_subst(ty_arg);

                // If the resolved type is still a TyVar, check if it's an unresolved
                // type variable (can't monomorphize) vs a concrete ADT name (can monomorphize).
                // Types like `Token` are represented as `TyVar("Token")` but they're concrete
                // types that should trigger monomorphization.
                if let Type::TyVar(name) = &resolved_ty_arg {
                    if !self.types.is_concrete_named_type(name) {
                        // It's an unresolved type variable - can't monomorphize
                        return self.compile_term(t);
                    }
                    // Otherwise it's a known ADT/record type - proceed to monomorphize
                }

                // Check if we need monomorphization
                if let Some(ty) = self.def_types.get(name).cloned() {
                    if matches!(ty, Type::Forall(_, _)) {
                        // This is a polymorphic function call
                        // Try to get or create monomorphized version
                        if let Some(specialized) =
                            self.compile_monomorphized(name, &resolved_ty_arg)?
                        {
                            return Ok(specialized);
                        }
                    }
                }
                // Fall back to erasure
                self.compile_term(t)
            }
            _ => {
                // Fall back to erasure for complex expressions
                self.compile_term(t)
            }
        }
    }

    /// Try to compile a monomorphized version of a polymorphic function.
    ///
    /// Returns Some(value) if we have the original term and can specialize it,
    /// or None if we should fall back to erasure.
    pub(crate) fn compile_monomorphized(
        &mut self,
        name: &str,
        ty_arg: &Type,
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodeGenError> {
        // Create a key for this monomorphization
        let ty_key = format!("{:?}", ty_arg); // Use debug format as a simple key
        let mono_key = (name.to_string(), ty_key.clone());

        // Check if we already have this monomorphized version
        if let Some(specialized_name) = self.monomorphized.get(&mono_key) {
            // Already compiled - return reference to it
            if let Some(func) = self.module.get_function(specialized_name) {
                return Ok(Some(self.wrap_function_as_closure(func)?));
            }
        }

        // Check if we have the original term
        let original_term = match self.term_defs.get(name) {
            Some(t) => t.clone(),
            None => return Ok(None), // No term available, fall back to erasure
        };

        // Get the original type
        let original_ty = match self.def_types.get(name) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        // Extract the type variable and body from TyAbs
        let (type_var, inner_term, inner_type) = match (&original_term, &original_ty) {
            (Term::TyAbs(var, body), Type::Forall(_, inner_ty)) => (
                var.clone(),
                body.as_ref().clone(),
                inner_ty.as_ref().clone(),
            ),
            _ => return Ok(None), // Not a polymorphic function
        };

        // Generate specialized name
        let specialized_name = format!("{}__mono_{}", name, self.counter);
        self.counter += 1;

        // Record this monomorphization (prevents re-compilation)
        self.monomorphized
            .insert(mono_key.clone(), specialized_name.clone());

        // Mark as being compiled (prevents infinite recursion for recursive functions)
        self.monomorphizing.insert(mono_key.clone());

        // Compute specialized type by substituting ty_arg for type_var
        let specialized_ty = inner_type.substitute(&type_var, ty_arg);

        // Declare the specialized function
        self.declare_def(&specialized_name, &specialized_ty)?;

        // Save all state before compiling the specialized function
        let saved_block = self.builder.get_insert_block();
        let saved_env = self.env.clone();
        let saved_current_fn = self.current_fn;
        let saved_type_subst = self.types.type_subst().clone();

        // Set up type substitution for compilation
        self.types.push_type_subst(type_var.clone(), ty_arg.clone());

        // Compile the specialized function
        let result = self.compile_def(&specialized_name, &inner_term, &specialized_ty);

        // Restore type substitution (don't just clear - preserve outer context)
        self.types.restore_type_subst(saved_type_subst);

        // Restore all saved state
        self.env = saved_env;
        self.current_fn = saved_current_fn;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }

        // Remove from monomorphizing set
        self.monomorphizing.remove(&mono_key);

        // Check compilation result
        result?;

        // Return reference to the specialized function
        if let Some(func) = self.module.get_function(&specialized_name) {
            Ok(Some(self.wrap_function_as_closure(func)?))
        } else {
            Err(CodeGenError::LlvmError(format!(
                "Failed to get monomorphized function {}",
                specialized_name
            )))
        }
    }

    /// Wrap a function value as a closure with null environment.
    fn wrap_function_as_closure(
        &self,
        func: inkwell::values::FunctionValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let closure_type = self
            .context
            .struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);
        let null_env = env_ptr_type.const_null();
        let mut closure = closure_type.const_zero();
        closure = self
            .builder
            .build_insert_value(
                closure,
                func.as_global_value().as_pointer_value(),
                0,
                "fn_ptr",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();
        closure = self
            .builder
            .build_insert_value(closure, null_env, 1, "null_env")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();
        Ok(closure.into())
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
    fn test_compile_ty_app_tyabs() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create TyAbs("T", NatLit(42))
        let term = Term::TyAbs("T".to_string(), Box::new(Term::NatLit(42)));

        // Compile TyApp(TyAbs, Nat)
        let result = codegen.compile_ty_app(&term, &Type::Nat).unwrap();
        assert!(result.is_int_value());
    }

    #[test]
    fn test_compile_ty_app_fallback() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // For non-TyAbs/non-Global terms, should fall back to compiling the term
        let term = Term::NatLit(42);
        let result = codegen.compile_ty_app(&term, &Type::Nat).unwrap();
        assert!(result.is_int_value());
    }

    #[test]
    fn test_wrap_function_as_closure() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        // Create a dummy function
        let ptr_type = context.ptr_type(AddressSpace::default());
        let fn_type = context.i64_type().fn_type(&[ptr_type.into()], false);
        let func = codegen.module.add_function("dummy_fn", fn_type, None);

        let closure = codegen.wrap_function_as_closure(func).unwrap();
        assert!(closure.is_struct_value());
        // Closure is { ptr, ptr }
        assert_eq!(closure.into_struct_value().get_type().count_fields(), 2);
    }

    #[test]
    fn test_monomorphization_not_polymorphic() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Register a non-polymorphic definition
        codegen.def_types.insert("not_poly".to_string(), Type::Nat);
        codegen
            .term_defs
            .insert("not_poly".to_string(), Term::NatLit(42));

        // Should return None (not polymorphic)
        let result = codegen
            .compile_monomorphized("not_poly", &Type::String)
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_monomorphization_no_term() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Register type but no term
        codegen.def_types.insert(
            "no_term".to_string(),
            Type::Forall("T".to_string(), Box::new(Type::TyVar("T".to_string()))),
        );

        // Should return None (no term available)
        let result = codegen
            .compile_monomorphized("no_term", &Type::Nat)
            .unwrap();
        assert!(result.is_none());
    }
}
