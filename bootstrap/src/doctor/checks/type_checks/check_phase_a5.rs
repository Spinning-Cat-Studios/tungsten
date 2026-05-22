//! `tungsten doctor check phase-a5` — verify Phase A.5 global collection health
//! (ADR 13.5.26g §2.3).
//!
//! Runs Phase A (type/constructor stubs) and Phase A.5 (combined AST +
//! global collection) and reports success or failure with source-level
//! diagnostics. Cost 3 (elaboration-level, no codegen).

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver::per_module::stubs;
use crate::driver::pipeline;
use crate::driver::{build_module_info, parse_module_tree};
use crate::elaborate::ModuleExports;
use tungsten_core::Context;

/// Entry point for `tungsten doctor check phase-a5 <file>`.
pub fn cmd_check_phase_a5(file: &PathBuf, verbose: bool) -> ExitCode {
    // Parse module tree
    let mut visited = std::collections::HashSet::new();
    let mut chain = Vec::new();
    let module_tree = match parse_module_tree(file, &mut visited, &mut chain, None) {
        Ok(tree) => tree,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let module_info = build_module_info(&module_tree);

    // Phase A: collect type + constructor stubs
    let mut exports = ModuleExports::default();
    stubs::collect_all_type_and_constructor_stubs(&module_tree, &mut exports);

    if verbose {
        eprintln!(
            "Phase A: {} types, {} constructors registered as stubs",
            exports.types.len(),
            exports.constructors.len(),
        );
    }

    // Phase A.5: build combined AST and run global collection
    let (combined_ast, combined_file_index) = pipeline::build_combined_ast(&module_tree);
    let mut combined_module_info = module_info.clone();
    combined_module_info.item_index_to_file = combined_file_index;

    let mut ctx = Context::new();
    match crate::elaborate::collect_definitions_with_exports(
        &combined_ast,
        &mut ctx,
        combined_module_info,
        &exports,
    ) {
        Ok(collected) => {
            let global_exports = collected.extract_value_exports();
            println!(
                "✓ Phase A.5 global collection succeeded: {} types, {} values, {} constructors",
                global_exports.types.len(),
                global_exports.values.len(),
                global_exports.constructors.len(),
            );
            ExitCode::SUCCESS
        }
        Err(errors) => {
            eprintln!(
                "✗ Phase A.5 global collection failed with {} error(s):\n",
                errors.len(),
            );
            for (i, e) in errors.iter().enumerate() {
                eprintln!("  {}. {}", i + 1, e);
            }
            eprintln!(
                "\nhint: fix the error(s) above, then re-run. These errors cause \
                 cross-module imports to fail silently during Phase B elaboration."
            );
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn project_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn check_phase_a5_on_clean_example() {
        let file = project_root().join("examples/hello.tg");
        let result = cmd_check_phase_a5(&file, false);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn check_phase_a5_fails_on_bad_import() {
        let file = project_root().join("tests/module_bugs/bad_import_phase_a5/main.tg");
        let result = cmd_check_phase_a5(&file, false);
        assert_eq!(result, ExitCode::FAILURE);
    }
}
