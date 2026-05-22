//! `tungsten doctor map-span` — convert byte offsets to file:line:col.
//!
//! Given a source file (or project main file) and a byte offset, reports
//! the line and column in the original source. When --project is used,
//! searches all module files to find which one contains the offset.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::span::LineIndex;

/// Run the map-span command.
///
/// Two modes:
/// 1. Single file: `tungsten doctor map-span <file> <offset>`
///    Converts byte offset to line:col in the specified file.
///
/// 2. Project search: `tungsten doctor map-span <main-file> <offset> --project`
///    Parses the module tree from main-file, then searches all module files
///    to find which one has content at the given byte offset, and reports
///    file:line:col.
pub fn cmd_map_span(file: &Path, offset: u32, project: bool, verbose: bool) -> ExitCode {
    if project {
        map_span_project(file, offset, verbose)
    } else {
        map_span_single(file, offset)
    }
}

/// Convert a byte offset to line:col in a single file.
fn map_span_single(file: &Path, offset: u32) -> ExitCode {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {}", file.display(), e);
            return ExitCode::FAILURE;
        }
    };

    if (offset as usize) > source.len() {
        eprintln!(
            "error: offset {} is beyond end of file ({} bytes)",
            offset,
            source.len()
        );
        return ExitCode::FAILURE;
    }

    let index = LineIndex::new(&source);
    let loc = index.location(offset);
    println!("{}:{}:{}", file.display(), loc.line, loc.column);

    // Show the source line for context
    if let Some(content) = index.line_content(&source, (loc.line - 1) as usize) {
        println!("  {}", content);
        // Show caret at column position
        let padding = " ".repeat((loc.column - 1) as usize);
        println!("  {}^", padding);
    }

    ExitCode::SUCCESS
}

/// Search all module files in a project for the byte offset.
fn map_span_project(main_file: &Path, offset: u32, verbose: bool) -> ExitCode {
    use std::collections::HashSet;

    // Parse module tree
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let module_tree = match crate::driver::modules::parse_module_tree(
        main_file,
        &mut visited,
        &mut chain,
        None,
    ) {
        Ok(tree) => tree,
        Err(e) => {
            eprintln!("error: failed to parse module tree: {e:?}");
            return ExitCode::FAILURE;
        }
    };

    // Collect all module files and their sources
    let source_map = crate::driver::modules::build_source_map(&module_tree);

    let mut found = false;
    for (path, source) in &source_map.sources {
        if (offset as usize) < source.len() {
            let index = LineIndex::new(source);
            let loc = index.location(offset);

            // Only report if we find valid content
            if verbose || !found {
                println!("{}:{}:{}", path.display(), loc.line, loc.column);
                if let Some(content) = index.line_content(source, (loc.line - 1) as usize) {
                    println!("  {}", content);
                    let padding = " ".repeat((loc.column - 1) as usize);
                    println!("  {}^", padding);
                }
                found = true;
                if !verbose {
                    // In non-verbose mode, only show first match
                    // (ambiguity: multiple files may have content at this offset)
                    break;
                }
            }
        }
    }

    if !found {
        eprintln!(
            "error: offset {} not found in any module file of {}",
            offset,
            main_file.display()
        );
        return ExitCode::FAILURE;
    }

    if verbose {
        eprintln!(
            "hint: use without --project for a specific file, or with -v to see all candidates"
        );
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn map_span_single_file() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "fn foo() -> Nat {{").unwrap();
        writeln!(f, "    42").unwrap();
        writeln!(f, "}}").unwrap();
        f.flush().unwrap();

        // Offset 0 → line 1, col 1
        let result = cmd_map_span(f.path(), 0, false, false);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn map_span_mid_file_offset() {
        let mut f = NamedTempFile::new().unwrap();
        // "fn foo() -> Nat {\n" = 18 bytes (including \n)
        // "    42\n"            = 7 bytes, starts at offset 18
        writeln!(f, "fn foo() -> Nat {{").unwrap();
        writeln!(f, "    42").unwrap();
        writeln!(f, "}}").unwrap();
        f.flush().unwrap();

        let source = std::fs::read_to_string(f.path()).unwrap();
        let index = LineIndex::new(&source);

        // Offset 18 = start of line 2 ("    42")
        let loc = index.location(18);
        assert_eq!(loc.line, 2);
        assert_eq!(loc.column, 1);

        // Offset 22 = '2' on line 2 (col 5)
        let loc2 = index.location(22);
        assert_eq!(loc2.line, 2);
        assert_eq!(loc2.column, 5);

        let result = cmd_map_span(f.path(), 22, false, false);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn map_span_at_newline() {
        let mut f = NamedTempFile::new().unwrap();
        // "ab\ncd\n" — \n at offsets 2 and 5
        write!(f, "ab\ncd\n").unwrap();
        f.flush().unwrap();

        let source = std::fs::read_to_string(f.path()).unwrap();
        let index = LineIndex::new(&source);

        // Offset 2 = the '\n' after "ab" → line 1, col 3
        let loc = index.location(2);
        assert_eq!(loc.line, 1);
        assert_eq!(loc.column, 3);

        let result = cmd_map_span(f.path(), 2, false, false);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn map_span_empty_file() {
        let f = NamedTempFile::new().unwrap();
        // Empty file: offset 0 is at end (len = 0), not beyond
        // offset 0 maps to line 1, col 1 (degenerate but valid)
        let result = cmd_map_span(f.path(), 0, false, false);
        assert_eq!(result, ExitCode::SUCCESS);

        // offset 1 is truly out of bounds
        let result = cmd_map_span(f.path(), 1, false, false);
        assert_eq!(result, ExitCode::FAILURE);
    }

    #[test]
    fn map_span_last_byte() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "hello").unwrap(); // 5 bytes, offsets 0–4
        f.flush().unwrap();

        let source = std::fs::read_to_string(f.path()).unwrap();
        let index = LineIndex::new(&source);

        // Offset 4 = 'o' → line 1, col 5
        let loc = index.location(4);
        assert_eq!(loc.line, 1);
        assert_eq!(loc.column, 5);

        let result = cmd_map_span(f.path(), 4, false, false);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn map_span_out_of_bounds() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "hi").unwrap();
        f.flush().unwrap();

        let result = cmd_map_span(f.path(), 999, false, false);
        assert_eq!(result, ExitCode::FAILURE);
    }
}
