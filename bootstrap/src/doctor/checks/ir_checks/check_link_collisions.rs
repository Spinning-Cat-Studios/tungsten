//! `tungsten doctor check link-collisions` — detect duplicate symbols across object files.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, ExitCode};

pub fn cmd_check_link_collisions(dir: &Path) -> ExitCode {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot read directory '{}': {}", dir.display(), e);
            return ExitCode::FAILURE;
        }
    };

    let object_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "o"))
        .collect();

    if object_files.is_empty() {
        eprintln!("error: no .o files found in '{}'", dir.display());
        return ExitCode::FAILURE;
    }

    // symbol_name -> list of files where it's defined
    let mut symbols: HashMap<String, Vec<String>> = HashMap::new();

    for obj in &object_files {
        let output = match Command::new("nm").arg("-g").arg(obj).output() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("error: failed to run nm on '{}': {}", obj.display(), e);
                return ExitCode::FAILURE;
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let file_name = obj
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        collect_defined_symbols(&stdout, &file_name, &mut symbols);
    }

    report_collisions(&symbols, object_files.len())
}

/// Parse `nm -g` output and collect defined text symbols.
///
/// Only tracks symbols with kind `T` or `t` (defined text symbols).
/// Undefined (`U`) and local symbols are ignored.
fn collect_defined_symbols(
    nm_output: &str,
    file_name: &str,
    symbols: &mut HashMap<String, Vec<String>>,
) {
    for line in nm_output.lines() {
        // nm output: "address kind name" or "         U name"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let kind = parts[1];
        let name = parts[2];
        // Only track defined text (T) symbols — these cause collisions
        if kind == "T" || kind == "t" {
            symbols
                .entry(name.to_string())
                .or_default()
                .push(file_name.to_string());
        }
    }
}

/// Report collisions and return appropriate exit code.
fn report_collisions(symbols: &HashMap<String, Vec<String>>, file_count: usize) -> ExitCode {
    let mut collisions: Vec<(&String, &Vec<String>)> = symbols
        .iter()
        .filter(|(_, files)| files.len() > 1)
        .collect();
    collisions.sort_by_key(|(name, _)| name.as_str());

    if collisions.is_empty() {
        println!(
            "No link collisions detected across {} object file(s).",
            file_count
        );
        return ExitCode::SUCCESS;
    }

    eprintln!(
        "Found {} symbol collision(s) across {} object file(s):",
        collisions.len(),
        file_count
    );
    eprintln!();
    for (name, files) in &collisions {
        eprintln!("  {} defined in:", name);
        for f in *files {
            eprintln!("    - {}", f);
        }
    }

    ExitCode::from(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_defined_symbols_parses_text_symbols() {
        let nm_output = "\
0000000000001000 T _main
0000000000002000 T _helper
0000000000003000 t _internal
                 U _printf
0000000000004000 D _data_sym";
        let mut symbols = HashMap::new();
        collect_defined_symbols(nm_output, "alpha.o", &mut symbols);

        assert_eq!(symbols.len(), 3);
        assert!(symbols.contains_key("_main"));
        assert!(symbols.contains_key("_helper"));
        assert!(symbols.contains_key("_internal"));
        // U and D symbols should not be collected
        assert!(!symbols.contains_key("_printf"));
        assert!(!symbols.contains_key("_data_sym"));
    }

    #[test]
    fn collect_defined_symbols_ignores_undefined() {
        let nm_output = "\
                 U _external_fn
                 U _another_extern";
        let mut symbols = HashMap::new();
        collect_defined_symbols(nm_output, "beta.o", &mut symbols);

        assert!(symbols.is_empty());
    }

    #[test]
    fn report_collisions_no_duplicates() {
        let mut symbols = HashMap::new();
        symbols.insert("_foo".to_string(), vec!["a.o".to_string()]);
        symbols.insert("_bar".to_string(), vec!["b.o".to_string()]);

        let result = report_collisions(&symbols, 2);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn report_collisions_with_duplicates() {
        let mut symbols = HashMap::new();
        symbols.insert(
            "_foo".to_string(),
            vec!["a.o".to_string(), "b.o".to_string()],
        );
        symbols.insert("_bar".to_string(), vec!["a.o".to_string()]);

        let result = report_collisions(&symbols, 2);
        assert_eq!(result, ExitCode::from(1));
    }

    #[test]
    fn collect_from_multiple_files_detects_collision() {
        let nm_a = "\
0000000000001000 T _shared_fn
0000000000002000 T _only_in_a";
        let nm_b = "\
0000000000001000 T _shared_fn
0000000000002000 T _only_in_b";

        let mut symbols = HashMap::new();
        collect_defined_symbols(nm_a, "a.o", &mut symbols);
        collect_defined_symbols(nm_b, "b.o", &mut symbols);

        // _shared_fn appears in both → collision
        assert_eq!(symbols["_shared_fn"].len(), 2);
        assert_eq!(symbols["_only_in_a"].len(), 1);
        assert_eq!(symbols["_only_in_b"].len(), 1);

        let result = report_collisions(&symbols, 2);
        assert_eq!(result, ExitCode::from(1));
    }
}
