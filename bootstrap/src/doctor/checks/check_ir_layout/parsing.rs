//! IR parsing and type-width analysis for store/load consistency checks.

pub(crate) struct StoreMismatch {
    pub line_number: usize,
    pub instruction: String,
    pub function: Option<String>,
    pub value_size: usize,
    pub pointer_size: usize,
}

/// Scan IR text for store instructions with type-width mismatches.
pub(crate) fn find_store_load_mismatches(ir: &str) -> Vec<StoreMismatch> {
    let mut mismatches = Vec::new();
    let mut current_function: Option<String> = None;

    for (i, line) in ir.lines().enumerate() {
        let trimmed = line.trim();

        // Track current function
        if trimmed.starts_with("define ") {
            if let Some(at_pos) = trimmed.find('@') {
                let after_at = &trimmed[at_pos + 1..];
                if let Some(paren_pos) = after_at.find('(') {
                    current_function = Some(after_at[..paren_pos].to_string());
                }
            }
        } else if trimmed == "}" {
            current_function = None;
        }

        // Check store instructions: store <ty> <val>, <ty>* <ptr>
        if trimmed.starts_with("store ") {
            if let Some(mismatch) = check_store_mismatch(trimmed, i + 1, &current_function) {
                mismatches.push(mismatch);
            }
        }
    }

    mismatches
}

/// Parse a store instruction and check for type-width mismatch.
///
/// Format: `store <value_type> <value>, <ptr_type>* <ptr>`
/// We compare the struct width of value_type vs the target of ptr_type.
fn check_store_mismatch(
    line: &str,
    line_number: usize,
    current_function: &Option<String>,
) -> Option<StoreMismatch> {
    let rest = line.strip_prefix("store ")?;

    // Split on the comma separating value and pointer
    let comma_pos = find_top_level_comma(rest)?;
    let value_part = rest[..comma_pos].trim();
    let ptr_part = rest[comma_pos + 1..].trim();

    // Extract value type (everything before the last space-separated token)
    let value_type = extract_type_prefix(value_part)?;

    // Extract pointer target type (strip trailing "* %..." or "* @...")
    let ptr_target_type = extract_ptr_target_type(ptr_part)?;

    let value_size = estimate_struct_size(&value_type);
    let ptr_size = estimate_struct_size(&ptr_target_type);

    // Only flag if both are struct types and sizes disagree
    if value_size > 0 && ptr_size > 0 && value_size != ptr_size {
        Some(StoreMismatch {
            line_number,
            instruction: line.to_string(),
            function: current_function.clone(),
            value_size,
            pointer_size: ptr_size,
        })
    } else {
        None
    }
}

/// Find the first comma not inside braces/brackets.
pub(crate) fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' | '[' | '(' | '<' => depth += 1,
            '}' | ']' | ')' | '>' => depth -= 1,
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Extract the type from a value operand like `{ i32, [24 x i8] } %val`.
pub(crate) fn extract_type_prefix(value_part: &str) -> Option<String> {
    let trimmed = value_part.trim();
    // For struct types: `{ ... } %name`
    if trimmed.starts_with('{') {
        let close = find_matching_brace(trimmed, 0)?;
        return Some(trimmed[..=close].to_string());
    }
    // For simple types: `i32 %name`, `i64 %name`
    let space = trimmed.rfind(' ')?;
    let ty = trimmed[..space].trim();
    if ty.is_empty() {
        None
    } else {
        Some(ty.to_string())
    }
}

/// Extract the target type from a pointer operand like `{ i32, [8 x i8] }* %ptr`.
pub(crate) fn extract_ptr_target_type(ptr_part: &str) -> Option<String> {
    let trimmed = ptr_part.trim();
    // For struct pointers: `{ ... }* %ptr`
    if trimmed.starts_with('{') {
        let close = find_matching_brace(trimmed, 0)?;
        // Check for * after the close brace
        let after = trimmed[close + 1..].trim_start();
        if after.starts_with('*') {
            return Some(trimmed[..=close].to_string());
        }
    }
    // For simple pointer types: `i32* %ptr`
    let star_pos = trimmed.find('*')?;
    let ty = trimmed[..star_pos].trim();
    if ty.is_empty() {
        None
    } else {
        Some(ty.to_string())
    }
}

/// Find the matching closing brace for an opening brace at `start`.
fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Estimate the byte size of an LLVM IR type.
///
/// Only handles simple cases: `i<N>`, `[N x <type>]`, `{ <types> }`.
/// Returns 0 for unknown types (we only flag when both sizes are known).
pub(crate) fn estimate_struct_size(ty: &str) -> usize {
    let trimmed = ty.trim();

    // Integer type: i8, i32, i64, etc.
    if let Some(bits_str) = trimmed.strip_prefix('i') {
        if let Ok(bits) = bits_str.parse::<usize>() {
            return bits.div_ceil(8);
        }
    }

    // Array type: [N x <type>]
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some(x_pos) = inner.find(" x ") {
            if let Ok(count) = inner[..x_pos].trim().parse::<usize>() {
                let elem_type = inner[x_pos + 3..].trim();
                let elem_size = estimate_struct_size(elem_type);
                if elem_size > 0 {
                    return count * elem_size;
                }
            }
        }
        return 0;
    }

    // Struct type: { <type>, <type>, ... }
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        let inner = &trimmed[1..trimmed.len() - 1].trim();
        if inner.is_empty() {
            return 0;
        }
        let fields = split_struct_fields(inner);
        let mut total = 0;
        for field in &fields {
            let sz = estimate_struct_size(field.trim());
            if sz == 0 {
                return 0; // Unknown field → can't estimate
            }
            total += sz;
        }
        return total;
    }

    // Pointer type or anything else: unknown
    0
}

/// Split struct fields by top-level commas (respecting nested braces/brackets).
pub(crate) fn split_struct_fields(s: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' | '[' | '(' | '<' => depth += 1,
            '}' | ']' | ')' | '>' => depth -= 1,
            ',' if depth == 0 => {
                fields.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    fields.push(&s[start..]);
    fields
}
