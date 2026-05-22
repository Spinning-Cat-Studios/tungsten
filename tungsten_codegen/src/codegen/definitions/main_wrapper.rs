//! Main entry-point wrapper compilation and output formatting.
//!
//! Contains `compile_main_wrapper` (emits `main` + `__tungsten_inner_main`)
//! and the `print_value` family of output helpers.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicValueEnum, FunctionValue};
use inkwell::AddressSpace;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile a main function wrapper.
    ///
    /// Emits two functions:
    /// - `__tungsten_inner_main`: contains signal handler setup, CLI args init,
    ///   the call to `tungsten_main`, and result printing.
    /// - `main`: stores argc/argv in globals, then calls
    ///   `__tungsten_inner_main` directly. musttail TCO (ADR 8.5.26c) makes
    ///   the 64 MB trampoline unnecessary.
    pub fn compile_main_wrapper(
        &mut self,
        main_ty: &Type,
    ) -> Result<FunctionValue<'ctx>, CodeGenError> {
        let i32_type = self.context.i32_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());

        let inner_main = self.emit_inner_main(main_ty, i32_type, ptr_type)?;
        self.emit_outer_main(inner_main, i32_type, ptr_type)
    }

    /// Emit `__tungsten_inner_main() -> i32`: signal handlers, args init,
    /// call to `tungsten_main`, and result printing.
    fn emit_inner_main(
        &mut self,
        main_ty: &Type,
        i32_type: inkwell::types::IntType<'ctx>,
        ptr_type: inkwell::types::PointerType<'ctx>,
    ) -> Result<FunctionValue<'ctx>, CodeGenError> {
        let inner_type = i32_type.fn_type(&[], false);
        let inner_main = self
            .module
            .add_function("__tungsten_inner_main", inner_type, None);

        let inner_entry = self.context.append_basic_block(inner_main, "entry");
        self.builder.position_at_end(inner_entry);

        self.emit_signal_handlers()?;
        self.emit_cli_args_init(i32_type, ptr_type)?;

        if let Some(tungsten_main) = self.module.get_function("tungsten_main") {
            let null_env = ptr_type.const_null();
            let result = self
                .builder
                .build_call(tungsten_main, &[null_env.into()], "main_result")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                .try_as_basic_value()
                .left();

            self.print_value(result, main_ty)?;
        }

        self.emit_alloc_profile_teardown()?;

        let zero = i32_type.const_int(0, false);
        self.builder
            .build_return(Some(&zero))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok(inner_main)
    }

    /// Install stack overflow signal handler (unless debug-info is active).
    fn emit_signal_handlers(&mut self) -> Result<(), CodeGenError> {
        if self.tracing.debug_info.is_none() {
            let handler_ty = self.context.void_type().fn_type(&[], false);
            let handler_fn =
                self.module
                    .add_function("__tungsten_install_signal_handlers", handler_ty, None);
            self.builder
                .build_call(handler_fn, &[], "")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }
        Ok(())
    }

    /// Initialize CLI args from globals.
    fn emit_cli_args_init(
        &mut self,
        i32_type: inkwell::types::IntType<'ctx>,
        ptr_type: inkwell::types::PointerType<'ctx>,
    ) -> Result<(), CodeGenError> {
        // Always init CLI args unconditionally. In per-module codegen,
        // tg_argc/tg_argv live in separate codegen units and are not visible
        // in the main wrapper's module, so a visibility check would skip
        // initialisation and leave CLI_ARGS empty (ADR 10.5.26c).
        let init_args_c_type = self
            .context
            .void_type()
            .fn_type(&[i32_type.into(), ptr_type.into()], false);
        let init_args_c = self
            .module
            .add_function("tg_init_args_c", init_args_c_type, None);

        let argc_global = self.module.add_global(i32_type, None, "__tungsten_argc");
        let argv_global = self.module.add_global(ptr_type, None, "__tungsten_argv");
        argc_global.set_initializer(&i32_type.const_int(0, false));
        argv_global.set_initializer(&ptr_type.const_null());

        let argc_val = self
            .builder
            .build_load(i32_type, argc_global.as_pointer_value(), "argc")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let argv_val = self
            .builder
            .build_load(ptr_type, argv_global.as_pointer_value(), "argv")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        self.builder
            .build_call(init_args_c, &[argc_val.into(), argv_val.into()], "")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok(())
    }

    /// Emit allocation profile teardown: set filter and print report (ADR 7.5.26b).
    fn emit_alloc_profile_teardown(&mut self) -> Result<(), CodeGenError> {
        if !self.tracing.alloc_profile {
            return Ok(());
        }

        if let Some(ref filter) = self.tracing.alloc_profile_filter {
            if !filter.is_empty() {
                if let Some(set_filter_fn) = self
                    .module
                    .get_function("__tungsten_alloc_profile_set_filter")
                {
                    let filter_ptr = self
                        .builder
                        .build_global_string_ptr(filter, "alloc_profile_filter")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                    self.builder
                        .build_call(set_filter_fn, &[filter_ptr.as_pointer_value().into()], "")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                }
            }
        }

        if let Some(report_fn) = self.module.get_function("__tungsten_alloc_profile_report") {
            self.builder
                .build_call(report_fn, &[], "")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }

        Ok(())
    }

    /// Emit `main(argc, argv) -> i32`: stores args in globals, then calls
    /// `__tungsten_inner_main()` directly.
    fn emit_outer_main(
        &mut self,
        inner_main: FunctionValue<'ctx>,
        i32_type: inkwell::types::IntType<'ctx>,
        ptr_type: inkwell::types::PointerType<'ctx>,
    ) -> Result<FunctionValue<'ctx>, CodeGenError> {
        let main_type = i32_type.fn_type(&[i32_type.into(), ptr_type.into()], false);
        let c_main = self.module.add_function("main", main_type, None);

        let entry = self.context.append_basic_block(c_main, "entry");
        self.builder.position_at_end(entry);

        let argc = c_main.get_nth_param(0).unwrap();
        let argv = c_main.get_nth_param(1).unwrap();

        // Store argc/argv into globals so __tungsten_inner_main can read them.
        // Always store unconditionally — in per-module codegen, tg_argc/tg_argv
        // live in separate codegen units and are not visible here (ADR 10.5.26c).
        let argc_global = self
            .module
            .get_global("__tungsten_argc")
            .expect("__tungsten_argc global should exist");
        let argv_global = self
            .module
            .get_global("__tungsten_argv")
            .expect("__tungsten_argv global should exist");

        self.builder
            .build_store(argc_global.as_pointer_value(), argc)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(argv_global.as_pointer_value(), argv)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Call __tungsten_inner_main directly on the default stack.
        // musttail TCO (ADR 8.5.26c) eliminates tail-recursive stack growth
        // for the hot recursive paths, making the 64 MB trampoline unnecessary.
        let result = self
            .builder
            .build_call(inner_main, &[], "exit_code")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .left()
            .unwrap();

        self.builder
            .build_return(Some(&result))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok(c_main)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Output formatting helpers
    // ─────────────────────────────────────────────────────────────────────────

    /// Print a value based on its type.
    pub(super) fn print_value(
        &mut self,
        value: Option<BasicValueEnum<'ctx>>,
        ty: &Type,
    ) -> Result<(), CodeGenError> {
        let printf = self.get_printf()?;

        match ty {
            Type::Nat => self.print_nat(printf, value),
            Type::Bool => self.print_bool(printf, value),
            Type::String => self.print_string(printf, value),
            Type::Unit => self.print_format(printf, "()\n", "unit_fmt"),
            _ => self.print_format(printf, "<value>\n", "value_fmt"),
        }
    }

    fn get_printf(&self) -> Result<inkwell::values::FunctionValue<'ctx>, CodeGenError> {
        self.module
            .get_function("printf")
            .ok_or_else(|| CodeGenError::LlvmError("printf not declared".to_string()))
    }

    fn print_nat(
        &mut self,
        printf: inkwell::values::FunctionValue<'ctx>,
        value: Option<BasicValueEnum<'ctx>>,
    ) -> Result<(), CodeGenError> {
        let fmt = self
            .builder
            .build_global_string_ptr("%lld\n", "nat_fmt")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(v) = value {
            self.builder
                .build_call(printf, &[fmt.as_pointer_value().into(), v.into()], "")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }
        Ok(())
    }

    fn print_bool(
        &mut self,
        printf: inkwell::values::FunctionValue<'ctx>,
        value: Option<BasicValueEnum<'ctx>>,
    ) -> Result<(), CodeGenError> {
        let true_str = self
            .builder
            .build_global_string_ptr("true\n", "true_str")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let false_str = self
            .builder
            .build_global_string_ptr("false\n", "false_str")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        if let Some(BasicValueEnum::IntValue(b)) = value {
            let str_ptr = self
                .builder
                .build_select(
                    b,
                    true_str.as_pointer_value(),
                    false_str.as_pointer_value(),
                    "bool_str",
                )
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            self.builder
                .build_call(printf, &[str_ptr.into()], "")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }
        Ok(())
    }

    fn print_string(
        &mut self,
        printf: inkwell::values::FunctionValue<'ctx>,
        value: Option<BasicValueEnum<'ctx>>,
    ) -> Result<(), CodeGenError> {
        let fmt = self
            .builder
            .build_global_string_ptr("%s\n", "str_fmt")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(BasicValueEnum::StructValue(s)) = value {
            let ptr = self
                .builder
                .build_extract_value(s, 0, "str_ptr")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            self.builder
                .build_call(printf, &[fmt.as_pointer_value().into(), ptr.into()], "")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }
        Ok(())
    }

    fn print_format(
        &mut self,
        printf: inkwell::values::FunctionValue<'ctx>,
        format: &str,
        name: &str,
    ) -> Result<(), CodeGenError> {
        let fmt = self
            .builder
            .build_global_string_ptr(format, name)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder
            .build_call(printf, &[fmt.as_pointer_value().into()], "")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    /// Verify that `compile_main_wrapper` unconditionally emits the CLI args
    /// init sequence — `tg_init_args_c` call + `__tungsten_argc`/`__tungsten_argv`
    /// globals — even when `tg_argc`/`tg_argv` are not present in the module.
    ///
    /// ADR 10.5.26c: in per-module codegen, `tg_argc`/`tg_argv` live in separate
    /// codegen units, so a visibility check would incorrectly skip init.
    #[test]
    fn test_emit_cli_args_init_unconditional() {
        let ctx = Context::create();
        let mut codegen = CodeGen::new(&ctx, "test_main_wrapper");

        // Register a simple main type: Unit -> Unit
        let main_ty = tungsten_core::types::Type::Arrow(
            Box::new(tungsten_core::types::Type::Unit),
            Box::new(tungsten_core::types::Type::Unit),
        );

        // Declare tungsten_main so the wrapper can call it
        codegen
            .declare_def("tungsten_main", &main_ty)
            .expect("declare tungsten_main");

        // Compile the main wrapper — should succeed without tg_argc/tg_argv
        codegen
            .compile_main_wrapper(&main_ty)
            .expect("compile_main_wrapper should succeed");

        let ir = codegen.get_ir_string();

        // Verify tg_init_args_c is declared and called
        assert!(
            ir.contains("tg_init_args_c"),
            "IR should contain tg_init_args_c declaration/call"
        );
        // Verify argc/argv globals are emitted
        assert!(
            ir.contains("__tungsten_argc"),
            "IR should contain __tungsten_argc global"
        );
        assert!(
            ir.contains("__tungsten_argv"),
            "IR should contain __tungsten_argv global"
        );
    }
}
