//! Three-way string comparison FFI (ADR 21.7.26c).
//!
//! Backs the self-host `StrMap<V>` AVL index used by the constructor-registration
//! dedup. Result encoding: 0 = LT, 1 = EQ, 2 = GT (the `CMP_LT`/`CMP_EQ`/
//! `CMP_GT` constants declared beside the `.tg` consumers).

use super::strings::TgString;
use std::cmp::Ordering;

/// Result value for "a sorts before b".
pub const STRING_CMP_LT: u64 = 0;
/// Result value for "a equals b".
pub const STRING_CMP_EQ: u64 = 1;
/// Result value for "a sorts after b".
pub const STRING_CMP_GT: u64 = 2;

/// Three-way byte-lexicographic comparison shared by the native FFI and the
/// evaluator's extern arm (identical semantics on both execution paths).
/// Byte-wise comparison coincides with `str::cmp` for UTF-8 strings.
#[must_use]
pub fn string_compare_bytes(a: &[u8], b: &[u8]) -> u64 {
    match a.cmp(b) {
        Ordering::Less => STRING_CMP_LT,
        Ordering::Equal => STRING_CMP_EQ,
        Ordering::Greater => STRING_CMP_GT,
    }
}

/// Three-way comparison of two Tungsten strings: 0 = LT, 1 = EQ, 2 = GT.
///
/// A null pointer is treated as the empty string (the `TgString` convention —
/// see `tg_cstring_to_string`), so null vs null is EQ and null vs non-empty
/// is LT.
///
/// # Safety
/// - `a.ptr`/`b.ptr` must each be valid for `a.len`/`b.len` bytes (or null)
#[no_mangle]
pub extern "C" fn tg_string_compare(a: TgString, b: TgString) -> u64 {
    let a_bytes = tg_string_as_bytes(&a);
    let b_bytes = tg_string_as_bytes(&b);
    string_compare_bytes(a_bytes, b_bytes)
}

/// View a `TgString` as a byte slice, treating null as empty.
fn tg_string_as_bytes(s: &TgString) -> &[u8] {
    if s.ptr.is_null() || s.len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(s.ptr.cast::<u8>(), s.len as usize) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_char;
    use std::ptr;

    fn tg(s: &str) -> TgString {
        TgString {
            ptr: s.as_ptr().cast::<c_char>(),
            len: s.len() as u64,
        }
    }

    fn tg_null() -> TgString {
        TgString {
            ptr: ptr::null(),
            len: 0,
        }
    }

    #[test]
    fn test_compare_equal() {
        assert_eq!(tg_string_compare(tg("abc"), tg("abc")), STRING_CMP_EQ);
    }

    #[test]
    fn test_compare_less_and_greater() {
        assert_eq!(tg_string_compare(tg("abc"), tg("abd")), STRING_CMP_LT);
        assert_eq!(tg_string_compare(tg("abd"), tg("abc")), STRING_CMP_GT);
    }

    #[test]
    fn test_compare_prefix_sorts_first() {
        assert_eq!(tg_string_compare(tg("ab"), tg("abc")), STRING_CMP_LT);
        assert_eq!(tg_string_compare(tg("abc"), tg("ab")), STRING_CMP_GT);
    }

    #[test]
    fn test_compare_empty_strings() {
        assert_eq!(tg_string_compare(tg(""), tg("")), STRING_CMP_EQ);
        assert_eq!(tg_string_compare(tg(""), tg("a")), STRING_CMP_LT);
        assert_eq!(tg_string_compare(tg("a"), tg("")), STRING_CMP_GT);
    }

    #[test]
    fn test_compare_null_is_empty() {
        assert_eq!(tg_string_compare(tg_null(), tg_null()), STRING_CMP_EQ);
        assert_eq!(tg_string_compare(tg_null(), tg("")), STRING_CMP_EQ);
        assert_eq!(tg_string_compare(tg_null(), tg("x")), STRING_CMP_LT);
        assert_eq!(tg_string_compare(tg("x"), tg_null()), STRING_CMP_GT);
    }

    #[test]
    fn test_compare_null_ptr_with_nonzero_len_is_empty() {
        // A null ptr is empty regardless of a (bogus) nonzero len — the
        // null check must short-circuit before the length is trusted.
        let bogus = TgString {
            ptr: ptr::null(),
            len: 5,
        };
        assert_eq!(tg_string_compare(bogus, tg("")), STRING_CMP_EQ);
        assert_eq!(tg_string_compare(bogus, tg("a")), STRING_CMP_LT);
        assert_eq!(tg_string_compare(tg("a"), bogus), STRING_CMP_GT);
    }

    #[test]
    fn test_compare_matches_str_cmp_on_utf8() {
        let cases = ["", "a", "ab", "b", "α", "β", "Type::ctor#0", "Type::ctor#1"];
        for x in &cases {
            for y in &cases {
                let expected = match x.cmp(y) {
                    Ordering::Less => STRING_CMP_LT,
                    Ordering::Equal => STRING_CMP_EQ,
                    Ordering::Greater => STRING_CMP_GT,
                };
                assert_eq!(tg_string_compare(tg(x), tg(y)), expected, "{x:?} vs {y:?}");
            }
        }
    }

    #[test]
    fn test_compare_bytes_not_length_first() {
        // Ordering is lexicographic by bytes, not by length: "b" > "aaaa".
        assert_eq!(tg_string_compare(tg("b"), tg("aaaa")), STRING_CMP_GT);
    }
}
