//! Top-level elaboration entry points.
//!
//! Convenience functions that create an `Elaborator`, run it, and return results.

use crate::ast::SourceFile;
use tungsten_core::Context;

use super::{CoreDef, ElabOutput};
use crate::elaborate::error::ElabError;
use crate::elaborate::Elaborator;

/// Elaborate a parsed source file to Core definitions.
///
/// This is the main entry point for elaboration. It:
/// 1. Collects all top-level definitions (first pass)
/// 2. Elaborates each definition to Core terms (second pass)
/// 3. Returns the elaborated definitions or accumulated errors
///
/// # Example
///
/// ```ignore
/// use tungsten_bootstrap::{parse, elaborate};
/// use tungsten_core::Context;
///
/// let source = "fn id(x: Nat) -> Nat { x }";
/// let (ast, parse_errors) = parse(source);
/// assert!(parse_errors.is_empty());
///
/// let mut ctx = Context::new();
/// match elaborate(&ast, &mut ctx) {
///     Ok(defs) => println!("Elaborated {} definitions", defs.len()),
///     Err(errors) => {
///         for e in errors {
///             eprintln!("Error: {}", e);
///         }
///     }
/// }
/// ```
pub fn elaborate(
    file: &SourceFile,
    core_ctx: &mut Context,
) -> Result<Vec<CoreDef>, Vec<ElabError>> {
    elaborate_with_warnings(file, core_ctx, None).map(|out| out.defs)
}

/// Elaborate a parsed source file to Core definitions, also returning warnings.
///
/// This is like `elaborate` but also returns warnings for non-fatal issues like
/// unreachable match arms. Warnings do not prevent compilation from succeeding.
pub fn elaborate_with_warnings(
    file: &SourceFile,
    core_ctx: &mut Context,
    trace_types: Option<String>,
) -> Result<ElabOutput, Vec<ElabError>> {
    elaborate_with_warnings_full(file, core_ctx, trace_types, None, None)
}

/// Elaborate a parsed source file to Core definitions with all trace options.
///
/// Supports both `--trace-types`, `--trace-encoding`, and `--trace-normalization`.
pub fn elaborate_with_warnings_full(
    file: &SourceFile,
    core_ctx: &mut Context,
    trace_types: Option<String>,
    trace_encoding: Option<String>,
    trace_normalization: Option<String>,
) -> Result<ElabOutput, Vec<ElabError>> {
    let mut elaborator = Elaborator::new(core_ctx);
    elaborator.set_trace_target(trace_types);
    elaborator.set_trace_encoding(trace_encoding);
    elaborator.set_trace_normalization(trace_normalization);
    match elaborator.elaborate_file(file) {
        Ok(defs) => Ok(ElabOutput {
            defs,
            warnings: std::mem::take(&mut elaborator.warnings),
            record_types: elaborator.get_record_types(),
            adt_types: elaborator.get_adt_types(),
            type_aliases: elaborator.get_type_aliases(),
            type_provenance: std::mem::take(&mut elaborator.type_provenance),
            encoded_types: elaborator.get_encoded_types(),
            mutual_recursion_groups: elaborator.get_mutual_recursion_groups(),
            type_visibilities: elaborator.get_type_visibilities(),
            record_field_visibilities: elaborator.get_record_field_visibilities(),
        }),
        Err(errors) => Err(errors),
    }
}

/// Run only the collection pass (first pass of elaboration).
///
/// This collects all type and value definitions into the environment,
/// which can then be used to compute a types_hash for IR caching.
pub fn collect_definitions<'a>(
    file: &SourceFile,
    core_ctx: &'a mut Context,
) -> Result<super::CollectedElaborator<'a>, Vec<ElabError>> {
    let mut elaborator = Elaborator::new(core_ctx);
    elaborator.run_collection_pass(file)?;
    Ok(super::CollectedElaborator {
        elaborator,
        file: file.clone(),
    })
}

/// Run the collection pass with module information.
///
/// Like `collect_definitions`, but populates the environment with module
/// information for qualified path resolution.
pub fn collect_definitions_with_modules<'a>(
    file: &SourceFile,
    core_ctx: &'a mut Context,
    module_info: crate::driver::modules::ModuleInfo,
) -> Result<super::CollectedElaborator<'a>, Vec<ElabError>> {
    let mut elaborator = Elaborator::new(core_ctx);

    // Populate module registry
    elaborator.env.populate_module_info(module_info);

    elaborator.run_collection_pass(file)?;
    Ok(super::CollectedElaborator {
        elaborator,
        file: file.clone(),
    })
}
