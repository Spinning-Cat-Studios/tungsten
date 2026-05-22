//! Text-level LLVM IR scanner for declaration hygiene.
//!
//! Collects `define @<name>` and `declare @<name>` symbols, then finds
//! direct `call ... @<name>(...)` targets missing from that set.

use std::collections::HashSet;

/// A missing declaration: a call target with no corresponding define/declare.
pub(crate) struct MissingDeclaration {
    pub symbol: String,
    pub line_number: usize,
}

/// Scan IR text and return all direct call targets lacking define/declare.
pub(crate) fn find_missing_declarations(ir: &str) -> Vec<MissingDeclaration> {
    let defined = collect_defined_symbols(ir);
    let calls = collect_call_targets(ir);

    let mut missing = Vec::new();
    for (symbol, line_number) in calls {
        if !defined.contains(symbol.as_str()) && !is_llvm_intrinsic(&symbol) {
            missing.push(MissingDeclaration {
                symbol,
                line_number,
            });
        }
    }
    missing
}

/// Collect all symbols that have a `define` or `declare` in this IR file.
fn collect_defined_symbols(ir: &str) -> HashSet<String> {
    let mut symbols = HashSet::new();
    for line in ir.lines() {
        let trimmed = line.trim();
        if let Some(name) = extract_define_or_declare(trimmed) {
            symbols.insert(name);
        }
    }
    symbols
}

/// Collect all direct call targets: `(symbol_name, line_number)` pairs.
fn collect_call_targets(ir: &str) -> Vec<(String, usize)> {
    let mut targets = Vec::new();
    for (i, line) in ir.lines().enumerate() {
        let trimmed = line.trim();
        for symbol in extract_call_targets(trimmed) {
            targets.push((symbol, i + 1));
        }
    }
    targets
}

/// Extract the symbol name from a `define` or `declare` line.
///
/// Matches patterns like:
/// - `define ... @symbol_name(...)`
/// - `declare ... @symbol_name(...)`
fn extract_define_or_declare(line: &str) -> Option<String> {
    let keyword = if line.starts_with("define ") {
        "define "
    } else if line.starts_with("declare ") {
        "declare "
    } else {
        return None;
    };

    // Find @symbol after the keyword and any linkage/return type
    let after_keyword = &line[keyword.len()..];
    let at_pos = after_keyword.find('@')?;
    let after_at = &after_keyword[at_pos + 1..];

    // Symbol ends at '(' or whitespace
    let end = after_at
        .find(|c: char| c == '(' || c.is_whitespace())
        .unwrap_or(after_at.len());
    let name = &after_at[..end];
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

/// Extract all direct call targets from a single line.
///
/// Matches `call ... @symbol(...)` and `invoke ... @symbol(...)`.
/// Does NOT match indirect calls through function pointers.
fn extract_call_targets(line: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut search_from = 0;

    while search_from < line.len() {
        let remaining = &line[search_from..];

        // Look for "call " or "invoke " followed eventually by @symbol
        let call_pos = remaining
            .find("call ")
            .or_else(|| remaining.find("invoke "));
        let Some(pos) = call_pos else { break };

        let after_call = &remaining[pos..];

        // Find @symbol in this call/invoke. Must appear before the next newline.
        if let Some(at_pos) = after_call.find('@') {
            let after_at = &after_call[at_pos + 1..];
            // Symbol ends at '(' — the argument list
            if let Some(paren_pos) = after_at.find('(') {
                let name = &after_at[..paren_pos];
                // Reject if name contains spaces (not a direct call symbol)
                if !name.is_empty() && !name.contains(' ') && !name.contains('%') {
                    targets.push(name.to_string());
                }
            }
            search_from += pos + at_pos + 1;
        } else {
            break;
        }
    }

    targets
}

/// Check if a symbol is an LLVM intrinsic (prefixed with `llvm.`).
fn is_llvm_intrinsic(name: &str) -> bool {
    name.starts_with("llvm.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_define() {
        assert_eq!(
            extract_define_or_declare("define i64 @my_func(i64 %0) {"),
            Some("my_func".to_string())
        );
    }

    #[test]
    fn extract_declare() {
        assert_eq!(
            extract_define_or_declare("declare ptr @malloc(i64)"),
            Some("malloc".to_string())
        );
    }

    #[test]
    fn extract_define_with_linkage() {
        assert_eq!(
            extract_define_or_declare("define internal ptr @helper$direct(ptr %0) {"),
            Some("helper$direct".to_string())
        );
    }

    #[test]
    fn extract_not_define() {
        assert_eq!(extract_define_or_declare("  %1 = add i64 %0, 1"), None);
    }

    #[test]
    fn extract_call_simple() {
        let targets = extract_call_targets("  %2 = call i64 @foo(i64 %1)");
        assert_eq!(targets, vec!["foo"]);
    }

    #[test]
    fn extract_call_no_at_sign() {
        let targets = extract_call_targets("  call void %fptr(i64 %1)");
        assert!(targets.is_empty(), "indirect calls should be ignored");
    }

    #[test]
    fn extract_invoke() {
        let targets =
            extract_call_targets("  invoke void @bar(i64 %0) to label %cont unwind label %cleanup");
        assert_eq!(targets, vec!["bar"]);
    }

    #[test]
    fn is_intrinsic() {
        assert!(is_llvm_intrinsic("llvm.memcpy.p0.p0.i64"));
        assert!(!is_llvm_intrinsic("malloc"));
    }
}
