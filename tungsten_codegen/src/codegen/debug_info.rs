//! Debug info generation (T1, ADR 16.4.26a).
//!
//! Provides definition-level DWARF debug info for Tungsten-compiled binaries.
//! This is the MVP implementation: each top-level definition gets a `DISubprogram`
//! with the correct source file and line number, enabling meaningful stack traces
//! in debuggers. Sub-expression locations are not yet tracked (requires span
//! propagation through the Term IR — see ADR for future work).

use super::CodeGen;
use inkwell::debug_info::{
    AsDIScope, DICompileUnit, DIFlags, DIFlagsConstants, DISubprogram, DISubroutineType,
    DWARFEmissionKind, DWARFSourceLanguage, DebugInfoBuilder,
};
use inkwell::values::FunctionValue;
use tungsten_core::terms::TermSpan;

/// Debug info state attached to a `CodeGen` instance.
pub(crate) struct DebugInfoState<'ctx> {
    pub(crate) di_builder: DebugInfoBuilder<'ctx>,
    pub(crate) compile_unit: DICompileUnit<'ctx>,
    /// Pre-computed line start byte offsets for efficient span→line conversion.
    line_starts: Vec<u32>,
}

impl<'ctx> DebugInfoState<'ctx> {
    /// Convert a byte offset into a 1-based line number.
    pub(crate) fn byte_offset_to_line(&self, offset: u32) -> u32 {
        // Binary search for the line containing this offset
        match self.line_starts.binary_search(&offset) {
            Ok(idx) => (idx + 1) as u32,
            Err(idx) => idx as u32,
        }
    }

    /// Create a generic subroutine type (no parameter/return type info).
    /// For the MVP, we don't encode Tungsten types as DWARF types.
    pub(crate) fn create_void_subroutine_type(&self) -> DISubroutineType<'ctx> {
        self.di_builder.create_subroutine_type(
            self.compile_unit.get_file(),
            None,
            &[],
            DIFlags::PUBLIC,
        )
    }

    /// Create a `DISubprogram` for a top-level function definition and attach it.
    pub(crate) fn create_function_debug_info(
        &self,
        name: &str,
        line: u32,
        function: FunctionValue<'ctx>,
    ) -> DISubprogram<'ctx> {
        let subroutine_type = self.create_void_subroutine_type();

        let di_subprogram = self.di_builder.create_function(
            self.compile_unit.as_debug_info_scope(),
            name,
            None, // linkage name — same as name
            self.compile_unit.get_file(),
            line,
            subroutine_type,
            true, // is_local_to_unit
            true, // is_definition
            line, // scope_line
            DIFlags::PUBLIC,
            false, // is_optimized
        );

        function.set_subprogram(di_subprogram);
        di_subprogram
    }

    /// Finalize debug info. Must be called before any code emission.
    pub(crate) fn finalize(&self) {
        self.di_builder.finalize();
    }
}

impl<'ctx> CodeGen<'ctx> {
    /// Enable debug info generation for this module.
    ///
    /// `source_path` is the primary source file path (used for DWARF file references).
    /// `source_text` is the full text of the source file (used for span→line conversion).
    pub fn enable_debug_info(&mut self, source_path: &str, source_text: &str) {
        use std::path::Path;

        // Extract filename and directory from source_path
        let path = Path::new(source_path);
        let filename = path.file_name().map_or_else(
            || source_path.to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
        let directory = path
            .parent()
            .map_or_else(|| ".".to_string(), |p| p.to_string_lossy().into_owned());

        // Set the module flag for debug info version
        let debug_metadata_version = self.context.i32_type().const_int(3, false);
        self.module.add_basic_value_flag(
            "Debug Info Version",
            inkwell::module::FlagBehavior::Warning,
            debug_metadata_version,
        );

        let (di_builder, compile_unit) = self.module.create_debug_info_builder(
            true,                   // allow_unresolved
            DWARFSourceLanguage::C, // closest available; no "Tungsten" DWARF lang
            &filename,
            &directory,
            "tungsten", // producer
            false,      // is_optimized
            "",         // flags
            0,          // runtime_ver
            "",         // split_name
            DWARFEmissionKind::Full,
            0,     // dwo_id
            false, // split_debug_inlining
            false, // debug_info_for_profiling
            "",    // sysroot (LLVM 11+)
            "",    // sdk (LLVM 11+)
        );

        // Pre-compute line starts for efficient offset→line lookup
        let line_starts = compute_line_starts(source_text);

        self.tracing.debug_info = Some(DebugInfoState {
            di_builder,
            compile_unit,
            line_starts,
        });
    }

    /// If debug info is enabled, create a `DISubprogram` for the given definition
    /// and set the current debug location. Returns the line number if debug info
    /// was emitted.
    pub(crate) fn attach_debug_info_to_def(
        &self,
        name: &str,
        span_start: u32,
        function: FunctionValue<'ctx>,
    ) {
        if let Some(ref di) = self.tracing.debug_info {
            let line = di.byte_offset_to_line(span_start);
            let subprogram = di.create_function_debug_info(name, line, function);

            // Set current debug location to the function's definition line
            let loc = di.di_builder.create_debug_location(
                self.context,
                line,
                0, // column (not tracked in MVP)
                subprogram.as_debug_info_scope(),
                None,
            );
            self.builder.set_current_debug_location(loc);
        }
    }

    /// Finalize debug info before code emission. No-op if debug info is disabled.
    pub fn finalize_debug_info(&self) {
        if let Some(ref di) = self.tracing.debug_info {
            di.finalize();
        }
    }

    /// Set the debug location for a sub-expression span (ADR 17.4.26a §3.1).
    ///
    /// If debug info is enabled, converts the span's start byte offset to a
    /// line number and sets the builder's current debug location. This gives
    /// sub-expression granularity in the debugger (instruction-level DWARF).
    pub(crate) fn set_debug_location_for_span(&self, span: &TermSpan) {
        if let Some(ref di) = self.tracing.debug_info {
            let line = di.byte_offset_to_line(span.start);
            if let Some(function) = self.compilation.current_fn {
                if let Some(subprogram) = function.get_subprogram() {
                    let loc = di.di_builder.create_debug_location(
                        self.context,
                        line,
                        0, // column (not tracked yet)
                        subprogram.as_debug_info_scope(),
                        None,
                    );
                    self.builder.set_current_debug_location(loc);
                }
            }
        }
    }
}

/// Compute byte offsets where each line starts. Line 1 starts at offset 0.
fn compute_line_starts(source: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    for (i, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push((i + 1) as u32);
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_line_starts() {
        let src = "line1\nline2\nline3";
        let starts = compute_line_starts(src);
        assert_eq!(starts, vec![0, 6, 12]);
    }

    #[test]
    fn test_compute_line_starts_empty() {
        let starts = compute_line_starts("");
        assert_eq!(starts, vec![0]);
    }

    #[test]
    fn test_compute_line_starts_trailing_newline() {
        let src = "a\nb\n";
        let starts = compute_line_starts(src);
        assert_eq!(starts, vec![0, 2, 4]);
    }
}
