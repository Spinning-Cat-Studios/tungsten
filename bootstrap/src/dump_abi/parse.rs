//! Signature parsing for LLVM IR function definitions.
//!
//! Extracts return types and parameter types from `define`/`declare` signatures,
//! handling nested braces, linkage keywords, and parameter attributes.

/// Parse function signature into return type and parameter types.
///
/// Input format: `define <ret_ty> @name(<param_tys>)` or `declare ...`
pub(super) fn parse_signature(sig: &str) -> Result<(String, Vec<String>), String> {
    // Find return type: between `define`/`declare` keyword and `@`
    let after_keyword = if let Some(rest) = sig.strip_prefix("define ") {
        rest
    } else if let Some(rest) = sig.strip_prefix("declare ") {
        rest
    } else {
        return Err(format!("unexpected signature format: {sig}"));
    };

    // Strip optional linkage/visibility keywords
    let after_keyword = strip_linkage(after_keyword);

    // Return type is everything up to the @
    let at_pos = after_keyword
        .find('@')
        .ok_or_else(|| format!("no '@' in signature: {sig}"))?;
    let ret_ty = after_keyword[..at_pos].trim().to_string();

    // Parameters are between the first '(' after '@' and the matching ')'
    let after_at = &after_keyword[at_pos + 1..];
    let open_paren = after_at
        .find('(')
        .ok_or_else(|| format!("no '(' in signature: {sig}"))?;
    let params_str = &after_at[open_paren + 1..];

    // Find matching close paren
    let close_paren = find_matching_paren(params_str)
        .ok_or_else(|| format!("unmatched '(' in signature: {sig}"))?;
    let params_str = &params_str[..close_paren];

    let param_tys = if params_str.trim().is_empty() {
        Vec::new()
    } else {
        split_params(params_str)
    };

    Ok((ret_ty, param_tys))
}

/// Strip common LLVM linkage/visibility keywords that appear before return type.
fn strip_linkage(s: &str) -> &str {
    let keywords = [
        "private ",
        "internal ",
        "external ",
        "linkonce ",
        "linkonce_odr ",
        "weak ",
        "weak_odr ",
        "common ",
        "appending ",
        "available_externally ",
        "hidden ",
        "protected ",
        "default ",
        "dso_local ",
        "dso_preemptable ",
        "unnamed_addr ",
        "local_unnamed_addr ",
    ];
    let mut result = s;
    loop {
        let prev = result;
        for kw in &keywords {
            if let Some(rest) = result.strip_prefix(kw) {
                result = rest;
            }
        }
        if result.len() == prev.len() {
            break;
        }
    }
    result
}

/// Find the position of the matching ')' in a string starting after '('.
fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Split parameter list by commas, respecting nested braces/brackets/parens.
fn split_params(s: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();

    for ch in s.chars() {
        match ch {
            '(' | '{' | '[' | '<' => {
                depth += 1;
                current.push(ch);
            }
            ')' | '}' | ']' | '>' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let param = extract_type_from_param(current.trim());
                if !param.is_empty() {
                    params.push(param);
                }
                current = String::new();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    let param = extract_type_from_param(current.trim());
    if !param.is_empty() {
        params.push(param);
    }

    params
}

/// Extract the type from a parameter declaration like `%S %arg0` or `i64 %val`.
/// Returns just the type portion (e.g., `%S`, `i64`).
fn extract_type_from_param(param: &str) -> String {
    let trimmed = param.trim();

    // Handle parameter attributes: noundef, signext, zeroext, etc.
    // These appear before the type.
    let trimmed = strip_param_attrs(trimmed);

    // If the param has a name (starts with %, or contains ' %'),
    // the type is everything before the last ` %` that isn't inside braces
    if let Some(name_start) = find_param_name_start(trimmed) {
        return trimmed[..name_start].trim().to_string();
    }

    // Otherwise the whole thing is the type (e.g., `...` for varargs)
    trimmed.to_string()
}

/// Strip parameter attributes that appear before the type.
fn strip_param_attrs(s: &str) -> &str {
    let attrs = [
        "noundef ",
        "signext ",
        "zeroext ",
        "inreg ",
        "byval ",
        "sret ",
        "nonnull ",
        "nocapture ",
        "readonly ",
        "writeonly ",
        "readnone ",
        "immarg ",
        "nest ",
        "returned ",
        "swiftself ",
        "swifterror ",
        "align ",
    ];
    let mut result = s;
    loop {
        let prev = result;
        for attr in &attrs {
            if let Some(rest) = result.strip_prefix(attr) {
                result = rest;
            }
        }
        if result.len() == prev.len() {
            break;
        }
    }
    result
}

/// Find where the parameter name starts (last ` %` not inside braces).
fn find_param_name_start(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut last_space_pct = None;
    let bytes = s.as_bytes();

    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' | b'{' | b'[' | b'<' => depth += 1,
            b')' | b'}' | b']' | b'>' => depth -= 1,
            b' ' if depth == 0 => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'%' {
                    last_space_pct = Some(i);
                }
            }
            _ => {}
        }
    }

    last_space_pct
}
