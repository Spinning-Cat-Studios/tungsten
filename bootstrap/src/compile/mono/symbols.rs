//! Symbol mangling for monomorphized instances and collision validation.
//!
//! Extends the length-prefixed mangling scheme (ADR 6.5.26c) with type-argument
//! encoding. Uses `_I` (Instantiation) separator between def path and type args.

use std::collections::HashMap;

use super::{MonoKey, MonoOwnershipMap};
use crate::compile::mangling::mangle_symbol;

/// Mangle a `MonoKey` into a linker-safe symbol name.
///
/// Format: `_tg_<module_path>_<name>_I_<type_arg_hash>`
///
/// The type-argument portion uses a stable string derived from the canonical
/// type args, with non-alphanumeric characters replaced to produce valid
/// linker symbols.
pub fn mangle_mono_symbol(key: &MonoKey) -> String {
    let path_refs: Vec<&str> = key.def_id.module_path.iter().map(|s| s.as_str()).collect();
    let base = mangle_symbol(&path_refs, &key.def_id.name);
    let sanitized_args = sanitize_type_args(&key.type_args.0);
    format!("{}_I_{}", base, sanitized_args)
}

/// Sanitize a canonical type-arg string for use in linker symbols.
///
/// Replaces non-alphanumeric characters with underscores, collapses runs,
/// and length-prefixes the result for collision safety.
fn sanitize_type_args(canonical: &str) -> String {
    let cleaned: String = canonical
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    // Collapse consecutive underscores
    let mut result = String::new();
    let mut prev_underscore = false;
    for c in cleaned.chars() {
        if c == '_' {
            if !prev_underscore {
                result.push('_');
            }
            prev_underscore = true;
        } else {
            result.push(c);
            prev_underscore = false;
        }
    }
    // Trim trailing underscore
    let trimmed = result.trim_end_matches('_');
    format!("{}{}", trimmed.len(), trimmed)
}

/// Validate the 3 collision-safety properties of the ownership map.
///
/// 1. Every `MonoKey` maps to exactly one symbol
/// 2. No two distinct `MonoKey`s map to the same symbol
/// 3. Each symbol maps to exactly one owner unit
///
/// Returns `Ok(())` or `Err(description)` for the first violation found.
pub fn validate_symbols(map: &MonoOwnershipMap) -> Result<(), String> {
    let entries = map.entries();

    // Property 1: every key → one symbol (guaranteed by HashMap)
    // Property 2: no symbol collision across keys
    let mut symbol_to_key: HashMap<&str, &MonoKey> = HashMap::new();
    for (key, ownership) in entries {
        if let Some(existing_key) = symbol_to_key.get(ownership.symbol.as_str()) {
            if *existing_key != key {
                return Err(format!(
                    "symbol collision: {} and {} both mangle to '{}'",
                    existing_key, key, ownership.symbol
                ));
            }
        }
        symbol_to_key.insert(&ownership.symbol, key);
    }

    // Property 3: each symbol → one owner
    let mut symbol_to_owner: HashMap<&str, &str> = HashMap::new();
    for ownership in entries.values() {
        if let Some(existing_owner) = symbol_to_owner.get(ownership.symbol.as_str()) {
            if *existing_owner != ownership.owner_unit.0 {
                return Err(format!(
                    "owner collision: symbol '{}' assigned to both '{}' and '{}'",
                    ownership.symbol, existing_owner, ownership.owner_unit
                ));
            }
        }
        symbol_to_owner.insert(&ownership.symbol, &ownership.owner_unit.0);
    }

    Ok(())
}
