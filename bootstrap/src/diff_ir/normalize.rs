//! IR normalization — canonicalize type defs and signatures for comparison.
//!
//! Strips SSA register numbers, metadata, and whitespace to make structural
//! comparison stable across different compilation runs.

/// Normalize a type definition for comparison.
///
/// Collapses whitespace variations.
pub(crate) fn normalize_type(def: &str) -> String {
    def.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Normalize a function signature for comparison.
///
/// Strips SSA register numbers (%0, %1, ... → %_) and metadata attributes
/// (nounwind, etc.) that don't affect ABI.
pub(crate) fn normalize_signature(sig: &str) -> String {
    let stripped = sig
        .replace(" nounwind", "")
        .replace(" readnone", "")
        .replace(" readonly", "")
        .replace(" willreturn", "");
    let ssa_normalized = strip_ssa_numbers(&stripped);
    ssa_normalized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Replace SSA register numbers (%0, %1, %42, ...) with a canonical placeholder (%_).
///
/// This ensures that two IR files with identical structure but different SSA
/// numbering (e.g., due to different compilation order) compare as equal.
fn strip_ssa_numbers(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
            // Found %<digit...> — replace with %_
            result.push_str("%_");
            i += 1; // skip '%'
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1; // skip digits
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_type_collapses_whitespace() {
        assert_eq!(normalize_type("{ i32,  [8 x i8] }"), "{ i32, [8 x i8] }");
    }

    #[test]
    fn normalize_signature_strips_attrs() {
        let sig = "define i32 @main() nounwind";
        assert_eq!(normalize_signature(sig), "define i32 @main()");
    }

    #[test]
    fn strip_ssa_numbers_replaces_numeric_registers() {
        assert_eq!(strip_ssa_numbers("%0"), "%_");
        assert_eq!(strip_ssa_numbers("%42"), "%_");
        assert_eq!(strip_ssa_numbers("%0, %1, %2"), "%_, %_, %_");
    }

    #[test]
    fn strip_ssa_numbers_preserves_named_registers() {
        assert_eq!(strip_ssa_numbers("%myvar"), "%myvar");
        assert_eq!(strip_ssa_numbers("%Item"), "%Item");
    }

    #[test]
    fn strip_ssa_numbers_preserves_type_refs() {
        // %Name (starts with letter) should be preserved
        assert_eq!(
            strip_ssa_numbers("%MyStruct = type { i32 }"),
            "%MyStruct = type { i32 }"
        );
    }

    #[test]
    fn normalize_signature_strips_ssa_and_attrs() {
        let a = "define i64 @main(ptr %0) nounwind";
        let b = "define i64 @main(ptr %1) willreturn";
        assert_eq!(normalize_signature(a), normalize_signature(b));
    }
}
