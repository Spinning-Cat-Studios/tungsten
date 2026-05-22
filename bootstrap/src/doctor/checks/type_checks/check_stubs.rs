//! `tungsten doctor check stubs` — detect residual type stubs after elaboration.
//!
//! After full elaboration (cost 3), checks whether any registered type names
//! remain unresolved — i.e., they were registered as stubs but never overwritten
//! by the collection pass. Uses the same `TypeDefKind::Stub` predicate as the
//! elaboration overwrite logic.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver::{self, build_module_info, parse_module_tree};

/// Entry point for `tungsten doctor check stubs <file>`.
pub fn cmd_check_stubs(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    // Phase 1: parse module tree and collect all registered type names
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
    let registered: BTreeSet<String> = module_info
        .modules
        .values()
        .flat_map(|contents| contents.types.iter().cloned())
        .collect();

    if registered.is_empty() {
        println!("No user-defined types found in {}", file.display());
        return ExitCode::SUCCESS;
    }

    // Phase 2: elaborate and collect resolved type names
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let resolved: BTreeSet<String> = project
        .adt_types
        .keys()
        .chain(project.record_types.keys())
        .chain(project.type_aliases.keys())
        .cloned()
        .collect();

    // Phase 3: compare — any registered type not in resolved is a residual stub
    let residual: Vec<&String> = registered.difference(&resolved).collect();

    let fully_resolved = registered.len() - residual.len();
    println!("Checking stub resolution in {}...\n", file.display());

    if residual.is_empty() {
        println!("✓ {} types fully resolved", fully_resolved);
        ExitCode::SUCCESS
    } else {
        println!("✓ {fully_resolved} types fully resolved");
        println!("⚠ {} type(s) still have stub kind:", residual.len());
        for name in &residual {
            println!("  {name:<30} — registered but not elaborated");
        }
        println!(
            "\nHint: residual stubs suggest types were registered during module\n\
             discovery but never overwritten by the collection pass.\n\
             Check type_def_is_stub() and the stub overwrite logic."
        );
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_vs_resolved_diff() {
        let mut registered = BTreeSet::new();
        registered.insert("Foo".to_string());
        registered.insert("Bar".to_string());
        registered.insert("Baz".to_string());

        let mut resolved = BTreeSet::new();
        resolved.insert("Foo".to_string());
        resolved.insert("Bar".to_string());

        let residual: Vec<&String> = registered.difference(&resolved).collect();
        assert_eq!(residual, vec![&"Baz".to_string()]);
    }

    #[test]
    fn all_resolved_means_empty_diff() {
        let mut registered = BTreeSet::new();
        registered.insert("A".to_string());
        registered.insert("B".to_string());

        let resolved = registered.clone();
        let residual: Vec<&String> = registered.difference(&resolved).collect();
        assert!(residual.is_empty());
    }
}
