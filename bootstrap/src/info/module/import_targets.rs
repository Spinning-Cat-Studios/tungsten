//! `tungsten info module import-targets` — show the value-import-target table
//! codegen uses to resolve colliding imported names (ADR 12.7.26a §2.1).
//!
//! For a module, lists each imported *value* name and the canonical defining
//! module codegen will bind it to — `Unambiguous(path)` (resolved) or
//! `Ambiguous([paths])` (a double-alias collision that hard-errors at
//! codegen). This is the table `collision_overrides_for_unit` consumes; it
//! answers "why did this colliding import resolve to X / why did it error?"
//! without reading `--emit-llvm`.

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::driver::{self, ValueImportTargetsByModule};
use tungsten_bootstrap::elaborate::ImportTarget;

/// Entry point for `tungsten info module import-targets <module> <file>`.
pub fn cmd_info_import_targets(
    module_path: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let key = module_key(module_path);
    println!(
        "Value import targets for {} in {}:\n",
        display_module(&key),
        file.display()
    );

    let Some(targets) = project.value_import_targets.get(&key) else {
        return report_no_targets(&project.value_import_targets);
    };

    render_targets(targets);
    // Ambiguous entries hard-error at codegen (D1), so gate the exit code on
    // them — this makes the command a usable pre-codegen ambiguity probe.
    let ambiguous = targets
        .values()
        .filter(|t| matches!(t, ImportTarget::Ambiguous(_)))
        .count();
    if ambiguous > 0 {
        println!(
            "\n⚠ {ambiguous} name(s) resolve ambiguously — these hard-error at codegen \
             (ADR 12.7.26a D1)."
        );
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Print the resolved table, one row per imported name.
fn render_targets(targets: &tungsten_bootstrap::elaborate::ValueImportTargets) {
    let max_name = targets.keys().map(String::len).max().unwrap_or(0);
    for (name, target) in targets {
        match target {
            ImportTarget::Unambiguous(path) => {
                println!("  {name:<max_name$}  →  {}", display_module(path));
            }
            ImportTarget::Ambiguous(paths) => {
                let candidates: Vec<String> = paths.iter().map(|p| display_module(p)).collect();
                println!(
                    "  {name:<max_name$}  ⚠  ambiguous: {}",
                    candidates.join(", ")
                );
            }
        }
    }
    println!("\n  {} imported value name(s).", targets.len());
}

/// Parse the module argument into a canonical-path key.
///
/// The entry file itself is the *root* module (empty path) — its imports key
/// under `[]`, shown as `<root>`. Since no `::`-split of a normal name yields
/// the empty path, accept `.`, `root`, `<root>`, or an empty string as the
/// root token so the entry file's table is addressable.
fn module_key(module_path: &str) -> Vec<String> {
    if matches!(module_path, "" | "." | "root" | "<root>") {
        Vec::new()
    } else {
        module_path.split("::").map(str::to_string).collect()
    }
}

/// No entry for this module: say so, and list the modules that do have one so
/// the user can spot a mistyped or non-canonical module path.
fn report_no_targets(all: &ValueImportTargetsByModule) -> ExitCode {
    println!("  (no imported values resolved for this module)");
    if !all.is_empty() {
        println!("\n  Modules with value import targets (query with the name shown):");
        for key in all.keys() {
            let hint = if key.is_empty() {
                "  (the entry file — query with `.`)"
            } else {
                ""
            };
            println!("    {}{hint}", display_module(key));
        }
        println!(
            "\n  Note: module paths are canonical (first-registered per file); a \
             workspace-sibling prefix like `main::parser` normalizes to `parser`."
        );
    }
    ExitCode::SUCCESS
}

/// Render a canonical module path (`<root>` for the empty path).
fn display_module(segments: &[String]) -> String {
    if segments.is_empty() {
        "<root>".to_string()
    } else {
        segments.join("::")
    }
}

#[cfg(test)]
mod tests {
    use super::{display_module, module_key};

    #[test]
    fn module_key_maps_root_tokens_to_empty_path() {
        for token in ["", ".", "root", "<root>"] {
            assert!(
                module_key(token).is_empty(),
                "`{token}` should address the root module"
            );
        }
    }

    #[test]
    fn module_key_splits_qualified_paths() {
        assert_eq!(module_key("main"), vec!["main".to_string()]);
        assert_eq!(
            module_key("elab::env::resolve_path"),
            vec![
                "elab".to_string(),
                "env".to_string(),
                "resolve_path".to_string()
            ]
        );
    }

    #[test]
    fn display_module_renders_root_and_paths() {
        assert_eq!(display_module(&[]), "<root>");
        assert_eq!(
            display_module(&["elab".to_string(), "env".to_string()]),
            "elab::env"
        );
    }
}
