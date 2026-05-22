//! ABI analysis: type layout resolution, passing mode decisions, and fuzzy matching.

use super::layout::{compute_layout, TypeLayout};
use super::parse::parse_signature;
use super::{FunctionAbi, ParamAbi, PassingMode, AAPCS64_DIRECT_THRESHOLD};
use crate::diff_ir::parser::IrDefs;

/// Analyze ABI for a single function given its signature and type table.
pub(super) fn analyze_function(
    name: &str,
    signature: &str,
    defs: &IrDefs,
) -> Result<FunctionAbi, String> {
    let (ret_ty, param_tys) = parse_signature(signature)?;

    let mut params = Vec::new();
    for (i, ty) in param_tys.iter().enumerate() {
        let param_name = format!("arg{i}");
        let layout = compute_type_layout(ty, &defs.types)?;
        let passing = determine_passing_mode(&layout);
        params.push(ParamAbi {
            name: param_name,
            ty: ty.clone(),
            layout,
            passing,
        });
    }

    let ret_layout = compute_type_layout(&ret_ty, &defs.types)?;
    let ret_passing = determine_passing_mode(&ret_layout);
    let ret = ParamAbi {
        name: "ret".to_string(),
        ty: ret_ty,
        layout: ret_layout,
        passing: ret_passing,
    };

    Ok(FunctionAbi {
        name: name.to_string(),
        signature: signature.to_string(),
        params,
        ret,
    })
}

/// Determine passing mode based on computed layout.
pub(super) fn determine_passing_mode(layout: &Option<TypeLayout>) -> PassingMode {
    match layout {
        Some(tl) => {
            if tl.total_size <= AAPCS64_DIRECT_THRESHOLD {
                PassingMode::Direct
            } else {
                PassingMode::Indirect
            }
        }
        // Primitives (i32, i64, ptr, etc.) are always direct
        None => PassingMode::Direct,
    }
}

/// Compute layout for a type string if it's a struct/array; returns None for primitives.
pub(super) fn compute_type_layout(
    ty: &str,
    type_table: &std::collections::HashMap<String, String>,
) -> Result<Option<TypeLayout>, String> {
    let trimmed = ty.trim();

    // Named type reference: resolve through type table
    if let Some(name) = trimmed.strip_prefix('%') {
        match type_table.get(name) {
            Some(def) => {
                // If it's a struct, compute its layout
                if def.starts_with('{') || def.starts_with("{ ") {
                    return compute_layout(def, type_table)
                        .map(Some)
                        .map_err(|e| format!("{e}"));
                }
                // Opaque type or other — try layout anyway
                return compute_layout(def, type_table)
                    .map(Some)
                    .map_err(|e| format!("resolving %{name}: {e}"));
            }
            None => {
                return Err(format!(
                    "unresolved type '%{name}' — not found in type table"
                ));
            }
        }
    }

    // Struct literal: { T1, T2, ... }
    if trimmed.starts_with('{') {
        return compute_layout(trimmed, type_table)
            .map(Some)
            .map_err(|e| format!("{e}"));
    }

    // Array: [N x T]
    if trimmed.starts_with('[') {
        return compute_layout(trimmed, type_table)
            .map(Some)
            .map_err(|e| format!("{e}"));
    }

    // Primitive types — no layout needed (direct pass)
    Ok(None)
}

/// Suggest similar function names when lookup fails.
pub(super) fn suggest_similar_functions(query: &str, defs: &IrDefs) {
    let mut candidates: Vec<(&String, usize)> = defs
        .functions
        .keys()
        .filter_map(|name| {
            let dist = levenshtein(query, name);
            if dist <= 3 || name.contains(query) || query.contains(name.as_str()) {
                Some((name, dist))
            } else {
                None
            }
        })
        .collect();

    candidates.sort_by_key(|(_, d)| *d);
    candidates.truncate(5);

    if !candidates.is_empty() {
        eprintln!("  did you mean:");
        for (name, _) in &candidates {
            eprintln!("    {name}");
        }
    }
}

/// Simple Levenshtein distance for fuzzy matching.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for (i, a_ch) in a.chars().enumerate() {
        for (j, b_ch) in b.chars().enumerate() {
            let cost = if a_ch == b_ch { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[a_len][b_len]
}
