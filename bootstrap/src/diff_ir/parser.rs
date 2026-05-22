//! LLVM IR parser — extracts type definitions and function signatures.
//!
//! Uses simple regex-based parsing (not a full IR parser).
//! Only extracts top-level `%name = type { ... }` and `define|declare` lines.

use std::collections::HashMap;

/// Parsed IR definitions.
pub(crate) struct IrDefs {
    /// Type name → type definition string
    pub types: HashMap<String, String>,
    /// Function name → signature string
    pub functions: HashMap<String, String>,
}

/// Parse type definitions and function signatures from LLVM IR text.
pub(crate) fn parse_ir_defs(ir: &str) -> IrDefs {
    let mut types = HashMap::new();
    let mut functions = HashMap::new();

    for line in ir.lines() {
        let trimmed = line.trim();

        // Type definition: %name = type { ... } or %name = type opaque
        if let Some(rest) = trimmed.strip_prefix('%') {
            if let Some(eq_pos) = rest.find(" = type ") {
                let name = rest[..eq_pos].to_string();
                let def = rest[eq_pos + " = type ".len()..].to_string();
                types.insert(name, def);
            }
        }

        // Function: define <ret> @name(<args>) or declare <ret> @name(<args>)
        if trimmed.starts_with("define ") || trimmed.starts_with("declare ") {
            if let Some(at_pos) = trimmed.find('@') {
                // Extract function name (up to '(')
                let after_at = &trimmed[at_pos + 1..];
                if let Some(paren_pos) = after_at.find('(') {
                    let name = after_at[..paren_pos].to_string();
                    // Signature is everything from return type to end of params
                    let sig_end = trimmed.find('{').unwrap_or(trimmed.len());
                    let signature = trimmed[..sig_end].trim().to_string();
                    functions.insert(name, signature);
                }
            }
        }
    }

    IrDefs { types, functions }
}
