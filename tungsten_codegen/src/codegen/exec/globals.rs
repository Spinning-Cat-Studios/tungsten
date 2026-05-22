//! Global and extern compilation.
//!
//! Handles compilation of:
//! - Global references (top-level functions and thunks)
//! - `ExternCall` (foreign function calls)

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::{BasicMetadataTypeEnum, BasicType};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};
use inkwell::AddressSpace;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile a global reference.
    ///
    /// For functions: wraps in a closure with null environment.
    /// For thunks (non-function defs): calls immediately with null env.
    pub(crate) fn compile_global(
        &mut self,
        name: &str,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Check if this name was remapped (extern wrappers are renamed)
        let lookup_name = self
            .defs
            .extern_name_map
            .get(name)
            .map_or(name, std::string::String::as_str);

        // Check for top-level function
        if let Some(func) = self.module.get_function(lookup_name) {
            // Check the type to determine if this is a function or a thunk
            // Use original name for type lookup since def_types uses original names
            if let Some(ty) = self
                .defs
                .def_types
                .get(lookup_name)
                .or_else(|| self.defs.def_types.get(name))
            {
                if !matches!(ty, Type::Arrow(_, _)) {
                    // Non-function (thunk): call it immediately with null env
                    let null_env = self.context.ptr_type(AddressSpace::default()).const_null();
                    let result = self
                        .builder
                        .build_call(func, &[null_env.into()], "thunk_call")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                    let result = result.try_as_basic_value().left().ok_or_else(|| {
                        CodeGenError::TypeError("thunk returned void".to_string())
                    })?;
                    // Materialize large struct results to fix ARM64 sret ABI issues
                    return self.materialize_call_result(result);
                }
            }

            // Function: wrap in closure with null env
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
            return Ok(closure.into());
        }

        Err(CodeGenError::Unsupported(format!(
            "global '{name}' not found (global variables not yet supported in codegen)"
        )))
    }

    /// Compile an external function call.
    ///
    /// Handles calling C FFI functions with proper ABI.
    pub(crate) fn compile_extern_call(
        &mut self,
        symbol: &str,
        compiled_args: Vec<BasicValueEnum<'ctx>>,
        ret_llvm: inkwell::types::BasicTypeEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Get the real C symbol name (strip __c_ prefix if present)
        let real_symbol = symbol.strip_prefix("__c_").unwrap_or(symbol);

        // Look up or declare the external function
        let func = if let Some(f) = self.module.get_function(symbol) {
            // Already declared with prefixed name
            f
        } else if let Some(f) = self.module.get_function(real_symbol) {
            // Check if it's a C extern (no basic blocks) or Tungsten wrapper (has body)
            let is_declaration = f.count_basic_blocks() == 0;
            let params_match = f.count_params() as usize == compiled_args.len();

            if is_declaration && params_match {
                f
            } else {
                // It's a wrapper or has wrong signature - use prefixed name
                let arg_types: Vec<BasicMetadataTypeEnum<'ctx>> =
                    compiled_args.iter().map(|a| a.get_type().into()).collect();
                let fn_type = ret_llvm.fn_type(&arg_types, false);
                self.module.add_function(symbol, fn_type, None)
            }
        } else {
            // Neither exists - declare the extern with the real C symbol name
            let arg_types: Vec<BasicMetadataTypeEnum<'ctx>> =
                compiled_args.iter().map(|a| a.get_type().into()).collect();

            let fn_type = ret_llvm.fn_type(&arg_types, false);
            self.module.add_function(real_symbol, fn_type, None)
        };

        // Convert args to metadata values for the call
        let call_args: Vec<BasicMetadataValueEnum<'ctx>> =
            compiled_args.iter().map(|v| (*v).into()).collect();

        let call = self
            .builder
            .build_call(func, &call_args, "extern_call")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Return value or unit if void
        if let Some(val) = call.try_as_basic_value().left() {
            self.materialize_call_result(val)
        } else {
            // Void return - return unit
            let unit_type = self.context.struct_type(&[], false);
            Ok(unit_type.const_named_struct(&[]).into())
        }
    }

    /// Compile a variable reference.
    ///
    /// First checks local environment, then top-level functions.
    pub(crate) fn compile_var(&mut self, x: &str) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // First check local environment
        if let Some((v, _)) = self.compilation.env.get(x) {
            return Ok(*v);
        }

        // Then check for top-level functions (need to wrap in closure)
        if let Some(func) = self.module.get_function(x) {
            // Wrap the function in a closure with null env
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
            return Ok(closure.into());
        }

        Err(CodeGenError::UnboundVariable(x.to_string()))
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
    fn test_compile_var_from_env() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Add a variable to the environment
        let val = context.i64_type().const_int(42, false);
        codegen.compilation.env.insert(
            "x".to_string(),
            (val.into(), tungsten_core::types::Type::Nat),
        );

        // Compile the variable
        let result = codegen.compile_var("x").unwrap();
        assert!(result.is_int_value());
    }

    #[test]
    fn test_compile_var_unbound() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        let result = codegen.compile_var("nonexistent");
        assert!(result.is_err());
        match result {
            Err(CodeGenError::UnboundVariable(name)) => {
                assert_eq!(name, "nonexistent");
            }
            _ => panic!("Expected UnboundVariable error"),
        }
    }

    #[test]
    fn test_compile_var_as_function() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Declare a function
        let ptr_type = context.ptr_type(AddressSpace::default());
        let fn_type = context.i64_type().fn_type(&[ptr_type.into()], false);
        codegen.module.add_function("my_func", fn_type, None);
        codegen
            .defs
            .def_types
            .insert("my_func".to_string(), Type::arrow(Type::Nat, Type::Nat));

        // Compile as variable - should wrap in closure
        let result = codegen.compile_var("my_func").unwrap();
        assert!(result.is_struct_value());
        // Closure is { ptr, ptr }
        assert_eq!(result.into_struct_value().get_type().count_fields(), 2);
    }

    #[test]
    fn test_compile_global_function() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Declare a function
        let ptr_type = context.ptr_type(AddressSpace::default());
        let fn_type = context.i64_type().fn_type(&[ptr_type.into()], false);
        codegen.module.add_function("global_fn", fn_type, None);
        codegen
            .defs
            .def_types
            .insert("global_fn".to_string(), Type::arrow(Type::Nat, Type::Nat));

        // Compile as global - should wrap in closure
        let result = codegen.compile_global("global_fn").unwrap();
        assert!(result.is_struct_value());
    }

    #[test]
    fn test_compile_global_not_found() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        let result = codegen.compile_global("nonexistent");
        assert!(result.is_err());
    }
}
