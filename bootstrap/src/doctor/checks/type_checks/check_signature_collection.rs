//! `tungsten doctor check module signature-collection` — verify Signature Collection global collection health
//! (ADR 13.5.26g §2.3).
//!
//! Runs Stub Registration (type/constructor stubs) and Signature Collection (combined AST +
//! global collection) and reports success or failure with source-level
//! diagnostics. Cost 3 (elaboration-level, no codegen).

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver::per_module::stubs;
use crate::driver::pipeline;
use crate::driver::{build_module_info, parse_module_tree};
use crate::elaborate::ModuleExports;
use tungsten_core::Context;

/// Entry point for `tungsten doctor check module signature-collection <file>`.
pub fn cmd_check_signature_collection(file: &PathBuf, verbose: bool) -> ExitCode {
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

    // Stub Registration: collect type + constructor stubs
    let mut exports = ModuleExports::default();
    stubs::collect_all_type_and_constructor_stubs(&module_tree, &mut exports);

    if verbose {
        eprintln!(
            "Stub Registration: {} types, {} constructors registered as stubs",
            exports.types.len(),
            exports.constructors.len(),
        );
    }

    // Signature Collection: build combined AST and run global collection
    let (combined_ast, combined_file_index) = pipeline::build_combined_ast(&module_tree);
    let mut combined_module_info = module_info.clone();
    combined_module_info.item_index_to_file = combined_file_index;

    let mut ctx = Context::new();
    // The same deferred entry point the live pass uses (ADR 14.8.26g D2), so
    // this check cannot disagree with what the driver actually did.
    match crate::elaborate::collect_definitions_for_signature_collection(
        &combined_ast,
        &mut ctx,
        combined_module_info,
        &exports,
    ) {
        Ok(mut collected) => {
            // The collection pass defers its errors instead of returning them
            // (ADR 14.8.26g D2), so a clean pass is `Ok` AND no deferred
            // errors — `Ok` alone would report a broken corpus as healthy.
            let errors = collected.take_collection_errors();
            if !errors.is_empty() {
                return report_collection_failure(&errors);
            }
            let global_exports = collected.extract_value_exports();
            println!(
                "✓ Signature Collection global collection succeeded: {} types, {} values, {} constructors",
                global_exports.types.len(),
                global_exports.values.len(),
                global_exports.constructors.len(),
            );
            ExitCode::SUCCESS
        }
        // Unreachable under the unconditional deferral; live again once ADR
        // 14.8.26g P3 restores the short-circuit for unpoisoned error sets.
        Err(errors) => report_collection_failure(&errors),
    }
}

/// Print the Signature Collection failure verdict, one numbered error per line.
fn report_collection_failure(errors: &[crate::elaborate::ElabError]) -> ExitCode {
    eprintln!(
        "✗ Signature Collection global collection failed with {} error(s):\n",
        errors.len(),
    );
    for (i, e) in errors.iter().enumerate() {
        eprintln!("  {}. {}", i + 1, e);
    }
    eprintln!(
        "\nhint: fix the error(s) above, then re-run. These errors cause \
         cross-module imports to fail silently during Body Elaboration elaboration."
    );
    ExitCode::FAILURE
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
    fn check_signature_collection_on_clean_example() {
        let file = project_root().join("examples/hello.tg");
        let result = cmd_check_signature_collection(&file, false);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn check_signature_collection_fails_on_bad_import() {
        let file = project_root().join("tests/module_bugs/bad_import_signature_collection/main.tg");
        let result = cmd_check_signature_collection(&file, false);
        assert_eq!(result, ExitCode::FAILURE);
    }

    /// A fixture that PARSES cleanly and fails during collection — the arm
    /// the bad-import fixture never reaches (it fails at the parse-tree
    /// stage). Since the deferral (ADR 14.8.26g D2) this is the
    /// `report_collection_failure` path behind the Ok-with-deferred-errors
    /// arm, and without this test its exit code scored no mutation coverage.
    #[test]
    fn a_deferred_collection_error_still_reports_failure() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.tg");
        std::fs::write(&file, "fn f() -> NoSuchType { 0 }").unwrap();
        assert_eq!(
            cmd_check_signature_collection(&file, false),
            ExitCode::FAILURE
        );
    }
}
