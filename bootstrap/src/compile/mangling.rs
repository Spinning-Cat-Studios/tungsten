//! Symbol mangling for per-module codegen (ADR 6.5.26c §2.4).
//!
//! **Status: Deferred (ADR 6.5.26d §2.3)**
//!
//! This module implements length-prefixed path-segment mangling but is NOT
//! currently wired into the codegen pipeline. The codegen layer resolves
//! functions by original name (`module.get_function("name")`), so applying
//! mangling requires a single name-remapping abstraction that maps original
//! names to mangled names at declaration/compilation time.
//!
//! **What unblocks this:**
//! - A name-remapping layer in codegen (likely introduced alongside
//!   single-owner monomorphization or W8 parallel codegen)
//! - All 4 name-resolution paths (extern_name_map, def_types, term_defs,
//!   module.get_function) must be updated to consult the remapping
//!
//! **Encoding scheme:** Uses length-prefixed path segments to avoid ambiguity
//! from underscores and nested paths. For example:
//!
//! ```text
//!   compiler::lexer::scan_token  →  _tg_8compiler_5lexer_10scan_token
//!   compiler::lexer_scan::token  →  _tg_8compiler_9lexer_scan_5token
//! ```

/// Mangle a symbol name using length-prefixed path segments.
///
/// Each segment is prefixed with its byte length, and the result is
/// prefixed with `_tg_`. This ensures collision safety even when
/// segment names contain underscores.
///
/// # Examples
///
/// ```
/// let mangled = mangle_symbol(&["compiler", "lexer"], "scan_token");
/// assert_eq!(mangled, "_tg_8compiler_5lexer_10scan_token");
/// ```
pub fn mangle_symbol(module_path: &[&str], name: &str) -> String {
    let mut result = String::from("_tg_");
    for segment in module_path {
        result.push_str(&segment.len().to_string());
        result.push_str(segment);
        result.push('_');
    }
    result.push_str(&name.len().to_string());
    result.push_str(name);
    result
}

/// Mangle a symbol name from owned string slices.
///
/// Like [`mangle_symbol`] but accepts `&[String]` for the module path,
/// which is the common type in the codegen pipeline.
#[allow(dead_code)] // scaffolding for module-aware codegen
pub fn mangle_symbol_owned(module_path: &[String], name: &str) -> String {
    let refs: Vec<&str> = module_path.iter().map(|s| s.as_str()).collect();
    mangle_symbol(&refs, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_top_level() {
        assert_eq!(mangle_symbol(&[], "main"), "_tg_4main");
    }

    #[test]
    fn test_single_module() {
        assert_eq!(
            mangle_symbol(&["lexer"], "scan_token"),
            "_tg_5lexer_10scan_token"
        );
    }

    #[test]
    fn test_nested_modules() {
        assert_eq!(
            mangle_symbol(&["compiler", "lexer"], "scan_token"),
            "_tg_8compiler_5lexer_10scan_token"
        );
    }

    #[test]
    fn test_collision_safety_underscore_in_name() {
        // These two must produce different mangled names:
        //   compiler::lexer::scan_token  vs  compiler::lexer_scan::token
        let a = mangle_symbol(&["compiler", "lexer"], "scan_token");
        let b = mangle_symbol(&["compiler", "lexer_scan"], "token");
        assert_ne!(a, b);
        assert_eq!(a, "_tg_8compiler_5lexer_10scan_token");
        assert_eq!(b, "_tg_8compiler_10lexer_scan_5token");
    }

    #[test]
    fn test_deep_nesting() {
        assert_eq!(
            mangle_symbol(&["a", "b", "c", "d"], "f"),
            "_tg_1a_1b_1c_1d_1f"
        );
    }

    #[test]
    fn test_long_names() {
        let long_name = "a".repeat(100);
        let mangled = mangle_symbol(&[&long_name], "x");
        assert_eq!(mangled, format!("_tg_100{}_1x", long_name));
    }

    #[test]
    fn test_constructor_name() {
        // ADT constructors: List::Cons
        assert_eq!(
            mangle_symbol(&["collections"], "Cons"),
            "_tg_11collections_4Cons"
        );
    }

    #[test]
    fn test_mangle_symbol_owned() {
        let path = vec!["compiler".to_string(), "lexer".to_string()];
        assert_eq!(
            mangle_symbol_owned(&path, "scan_token"),
            "_tg_8compiler_5lexer_10scan_token"
        );
    }

    #[test]
    fn test_empty_name() {
        // Edge case: empty name (shouldn't happen in practice)
        assert_eq!(mangle_symbol(&["mod"], ""), "_tg_3mod_0");
    }

    #[test]
    fn test_numeric_segment_names() {
        // Segment names that look like numbers
        assert_eq!(mangle_symbol(&["m123"], "fn456"), "_tg_4m123_5fn456");
    }

    #[test]
    fn test_uniqueness_systematic() {
        // Verify several potential collision pairs are all distinct
        let cases = vec![
            (vec!["ab", "cd"], "ef"),
            (vec!["abc", "d"], "ef"),
            (vec!["a", "bcd"], "ef"),
            (vec!["ab", "c", "d"], "ef"),
            (vec!["ab_cd"], "ef"),
            (vec!["ab"], "cd_ef"),
        ];
        let mangled: Vec<String> = cases
            .iter()
            .map(|(path, name)| mangle_symbol(path, name))
            .collect();
        // All must be distinct
        for i in 0..mangled.len() {
            for j in (i + 1)..mangled.len() {
                assert_ne!(
                    mangled[i], mangled[j],
                    "collision: {:?} == {:?} (cases {} and {})",
                    cases[i], cases[j], i, j
                );
            }
        }
    }
}
