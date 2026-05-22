//! Parser for `--dump-core-def` output format.
//!
//! Extracts definition entries from the box-drawing table format produced
//! by `tungsten compile --dump-ir=*`.

use std::collections::HashMap;

/// A parsed Core IR definition from a dump file.
#[derive(Debug, Clone)]
pub struct CoreDef {
    pub name: String,
    pub ty: String,
    #[allow(dead_code)] // Phase 3B: semantic type display
    pub semantic_ty: Option<String>,
    pub term: String,
    #[allow(dead_code)] // Phase 3B: TyVar divergence classification
    pub free_tyvars: String,
}

/// All definitions parsed from a Core IR dump file.
#[derive(Debug)]
pub struct CoreDefs {
    pub defs: HashMap<String, CoreDef>,
}

/// Parse `--dump-core-def` output into structured definitions.
///
/// The format uses box-drawing characters:
/// ```text
/// ┌─────────────────────────────────────────────────────────────┐
/// │  Definition: name                                          │
/// │  Type: ty                                                  │
/// │        = structural_ty                                     │
/// │                                                            │
/// │  Term: term                                                │
/// │  Free TyVars: ∅                                            │
/// └─────────────────────────────────────────────────────────────┘
/// ```
pub fn parse_core_defs(text: &str) -> CoreDefs {
    let mut defs = HashMap::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        // Look for definition start (box top)
        if lines[i].starts_with('┌') {
            if let Some((def, end)) = parse_one_def(&lines, i) {
                defs.insert(def.name.clone(), def);
                i = end;
                continue;
            }
        }
        i += 1;
    }

    CoreDefs { defs }
}

fn parse_one_def(lines: &[&str], start: usize) -> Option<(CoreDef, usize)> {
    let mut name = String::new();
    let mut ty = String::new();
    let mut semantic_ty: Option<String> = None;
    let mut term = String::new();
    let mut free_tyvars = String::new();

    let mut i = start + 1; // Skip the ┌ line

    while i < lines.len() {
        let line = lines[i].trim();

        if line.starts_with('└') {
            // End of definition
            return Some((
                CoreDef {
                    name: name.trim().to_string(),
                    ty: ty.trim().to_string(),
                    semantic_ty: semantic_ty.map(|s| s.trim().to_string()),
                    term: term.trim().to_string(),
                    free_tyvars: free_tyvars.trim().to_string(),
                },
                i + 1,
            ));
        }

        // Strip box-drawing border
        let content = strip_box_border(line);

        if let Some(rest) = content.strip_prefix("Definition: ") {
            name = rest.to_string();
        } else if let Some(rest) = content.strip_prefix("Type: ") {
            // First Type: line might be semantic, second (= ...) is structural
            ty = rest.to_string();
        } else if let Some(rest) = content.strip_prefix("= ") {
            // Structural type (when semantic type is on Type: line)
            semantic_ty = Some(ty.clone());
            ty = rest.to_string();
        } else if let Some(rest) = content.strip_prefix("Term: ") {
            term = rest.to_string();
        } else if let Some(rest) = content.strip_prefix("Free TyVars: ") {
            free_tyvars = rest.to_string();
        }

        i += 1;
    }

    None
}

/// Strip box-drawing borders from a line: │  content  │ → content
fn strip_box_border(line: &str) -> &str {
    let trimmed = line.trim();
    let stripped = trimmed.strip_prefix('│').unwrap_or(trimmed);
    let stripped = stripped.trim().trim_end_matches('│').trim();
    stripped
}
