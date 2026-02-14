//! LLVM backend - object file generation and IR output.

use super::error::CodeGenError;
use super::CodeGen;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::OptimizationLevel;
use std::path::Path;

impl<'ctx> CodeGen<'ctx> {
    /// Write the module to an object file.
    pub fn write_object_file(&self, path: &Path) -> Result<(), CodeGenError> {
        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        let target_triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&target_triple)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        let target_machine = target
            .create_target_machine(
                &target_triple,
                "generic",
                "",
                OptimizationLevel::Default,
                RelocMode::PIC,
                CodeModel::Default,
            )
            .ok_or_else(|| {
                CodeGenError::LlvmError("could not create target machine".to_string())
            })?;

        target_machine
            .write_to_file(&self.module, FileType::Object, path)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok(())
    }

    /// Print the LLVM IR to stderr (for debugging).
    pub fn dump_ir(&self) {
        self.module.print_to_stderr();
    }

    /// Get the LLVM IR as a string.
    pub fn get_ir_string(&self) -> String {
        self.module.print_to_string().to_string()
    }
}
