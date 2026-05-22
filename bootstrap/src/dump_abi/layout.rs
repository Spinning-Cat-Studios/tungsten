//! Struct layout computation for LLVM IR types.
//!
//! Computes field offsets, sizes, alignments, and padding for the narrow
//! subset of LLVM IR types that tungsten actually emits. Unsupported types
//! (vectors, packed structs, i128, etc.) fail fast with a clear error.

use std::collections::{HashMap, HashSet};

/// Maximum nesting depth for type resolution (prevents stack overflow on pathological IR).
const MAX_RESOLVE_DEPTH: u32 = 64;

/// Computed layout for a single field.
#[derive(Debug, Clone)]
pub(crate) struct FieldLayout {
    /// LLVM IR type string for this field.
    pub ty: String,
    /// Byte offset from the start of the struct.
    pub offset: u64,
    /// Size in bytes.
    pub size: u64,
    /// Alignment in bytes.
    pub align: u64,
}

/// Computed layout for a struct or array type.
#[derive(Debug, Clone)]
pub(crate) struct TypeLayout {
    /// Per-field layout info (empty for primitives/arrays).
    pub fields: Vec<FieldLayout>,
    /// Total size in bytes (including padding).
    pub total_size: u64,
    /// Maximum alignment requirement.
    pub max_align: u64,
    /// Total padding bytes.
    pub padding: u64,
    /// The original type string.
    pub type_str: String,
}

/// Layout computation error.
#[derive(Debug, Clone)]
pub(crate) enum LayoutError {
    UnsupportedType(String),
    UnresolvedType(String),
    CycleDetected(String),
    DepthExceeded,
    ParseError(String),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedType(ty) => {
                write!(
                    f,
                    "unsupported type '{ty}'\n  hint: use --deep for LLVM's actual ABI lowering"
                )
            }
            Self::UnresolvedType(name) => {
                write!(f, "unresolved type '%{name}' — not found in type table")
            }
            Self::CycleDetected(name) => {
                write!(f, "direct struct cycle detected through '%{name}' (only pointer-indirected cycles are supported)")
            }
            Self::DepthExceeded => {
                write!(
                    f,
                    "type nesting depth exceeded ({MAX_RESOLVE_DEPTH} levels)"
                )
            }
            Self::ParseError(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

/// Compute layout for an LLVM IR type string.
///
/// The `type_table` maps named types (`%Name`) to their definitions.
pub(crate) fn compute_layout(
    ty: &str,
    type_table: &HashMap<String, String>,
) -> Result<TypeLayout, LayoutError> {
    let mut seen = HashSet::new();
    compute_layout_inner(ty, type_table, &mut seen, 0)
}

fn compute_layout_inner(
    ty: &str,
    type_table: &HashMap<String, String>,
    seen: &mut HashSet<String>,
    depth: u32,
) -> Result<TypeLayout, LayoutError> {
    if depth > MAX_RESOLVE_DEPTH {
        return Err(LayoutError::DepthExceeded);
    }

    let trimmed = ty.trim();

    // Named type reference
    if let Some(name) = trimmed.strip_prefix('%') {
        if seen.contains(name) {
            return Err(LayoutError::CycleDetected(name.to_string()));
        }
        let def = type_table
            .get(name)
            .ok_or_else(|| LayoutError::UnresolvedType(name.to_string()))?;
        seen.insert(name.to_string());
        let result = compute_layout_inner(def, type_table, seen, depth + 1);
        seen.remove(name);
        return result.map(|mut layout| {
            layout.type_str = trimmed.to_string();
            layout
        });
    }

    // Struct: { T1, T2, ... }
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return compute_struct_layout(trimmed, type_table, seen, depth);
    }

    // Array: [N x T]
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return compute_array_layout(trimmed, type_table, seen, depth);
    }

    // Packed struct: <{ ... }> — unsupported
    if trimmed.starts_with("<{") || trimmed.starts_with("< {") {
        return Err(LayoutError::UnsupportedType(trimmed.to_string()));
    }

    // Vector: <N x T> — unsupported
    if trimmed.starts_with('<') {
        return Err(LayoutError::UnsupportedType(trimmed.to_string()));
    }

    // Primitive types
    if let Some(layout) = primitive_layout(trimmed) {
        return Ok(layout);
    }

    Err(LayoutError::UnsupportedType(trimmed.to_string()))
}

/// Get layout for a primitive LLVM IR type.
fn primitive_layout(ty: &str) -> Option<TypeLayout> {
    let (size, align) = match ty {
        "i1" | "i8" => (1, 1),
        "i16" => (2, 2),
        "i32" => (4, 4),
        "i64" => (8, 8),
        "ptr" => (8, 8),
        "float" => (4, 4),
        "double" => (8, 8),
        "void" => (0, 1),
        _ => return None,
    };

    Some(TypeLayout {
        fields: Vec::new(),
        total_size: size,
        max_align: align,
        padding: 0,
        type_str: ty.to_string(),
    })
}

/// Compute layout for a struct type: `{ T1, T2, ... }`
fn compute_struct_layout(
    ty: &str,
    type_table: &HashMap<String, String>,
    seen: &mut HashSet<String>,
    depth: u32,
) -> Result<TypeLayout, LayoutError> {
    // Strip outer braces
    let inner = ty
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| LayoutError::ParseError(format!("invalid struct type: {ty}")))?
        .trim();

    // Empty struct
    if inner.is_empty() {
        return Ok(TypeLayout {
            fields: Vec::new(),
            total_size: 0,
            max_align: 1,
            padding: 0,
            type_str: ty.to_string(),
        });
    }

    let field_types = split_type_list(inner);
    let mut fields = Vec::new();
    let mut offset: u64 = 0;
    let mut max_align: u64 = 1;
    let mut total_field_size: u64 = 0;

    for field_ty in &field_types {
        let field_layout = compute_layout_inner(field_ty, type_table, seen, depth + 1)?;

        let field_align = field_layout.max_align;
        // Align the offset
        let aligned_offset = align_to(offset, field_align);

        fields.push(FieldLayout {
            ty: field_ty.clone(),
            offset: aligned_offset,
            size: field_layout.total_size,
            align: field_align,
        });

        offset = aligned_offset + field_layout.total_size;
        total_field_size += field_layout.total_size;
        max_align = max_align.max(field_align);
    }

    // Round total size up to struct alignment
    let total_size = align_to(offset, max_align);
    let padding = total_size - total_field_size;

    Ok(TypeLayout {
        fields,
        total_size,
        max_align,
        padding,
        type_str: ty.to_string(),
    })
}

/// Compute layout for an array type: `[N x T]`
fn compute_array_layout(
    ty: &str,
    type_table: &HashMap<String, String>,
    seen: &mut HashSet<String>,
    depth: u32,
) -> Result<TypeLayout, LayoutError> {
    let inner = ty
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| LayoutError::ParseError(format!("invalid array type: {ty}")))?
        .trim();

    // Parse "N x T"
    let x_pos = inner
        .find(" x ")
        .ok_or_else(|| LayoutError::ParseError(format!("invalid array type (no ' x '): {ty}")))?;

    let count_str = inner[..x_pos].trim();
    let elem_ty = inner[x_pos + 3..].trim();

    let count: u64 = count_str.parse().map_err(|_| {
        LayoutError::ParseError(format!("invalid array count '{count_str}' in {ty}"))
    })?;

    let elem_layout = compute_layout_inner(elem_ty, type_table, seen, depth + 1)?;

    Ok(TypeLayout {
        fields: Vec::new(),
        total_size: count * elem_layout.total_size,
        max_align: elem_layout.max_align,
        padding: 0,
        type_str: ty.to_string(),
    })
}

/// Split a comma-separated type list, respecting nested braces/brackets.
fn split_type_list(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();

    for ch in s.chars() {
        match ch {
            '{' | '[' | '(' | '<' => {
                depth += 1;
                current.push(ch);
            }
            '}' | ']' | ')' | '>' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current = String::new();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }

    result
}

/// Align `offset` up to the next multiple of `align`.
fn align_to(offset: u64, align: u64) -> u64 {
    if align == 0 {
        return offset;
    }
    (offset + align - 1) & !(align - 1)
}
