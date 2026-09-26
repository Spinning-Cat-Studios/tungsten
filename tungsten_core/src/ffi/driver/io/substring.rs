//! `substring`, the **length** reading (ADR 20.8.26c).
//!
//! This is the runtime counterpart of `Term::StrSubstring`, and the symbol the
//! self-hosted elaborator's CIR mirror names. Its third argument is a **count of
//! bytes**, not an end offset.
//!
//! Its sibling `tg_string_slice` in `strings.rs` takes an **end index**
//! instead. The two are one word apart and
//! the difference is invisible at every call site — which is the whole subject
//! of ADR 20.8.26c, where a `.tg` `substring` reading its third argument as an
//! end index shadowed a builtin reading it as a length, and returned `""` for
//! every identifier whose offset exceeded its own length. They live in separate
//! files, under separate names, for that reason.

use std::ptr;

use super::strings::TgString;

/// Take `len` bytes of a Tungsten String starting at `start`.
///
/// Borrows into the original buffer — no allocation, and the result must not be
/// freed separately. Both arguments are clamped, so an out-of-range `start`
/// yields the empty string rather than reading past the end.
///
/// # Safety
/// - `s.ptr` must be a valid pointer to at least `s.len` bytes
#[no_mangle]
pub extern "C" fn tg_string_substring(s: TgString, start: u64, len: u64) -> TgString {
    if s.ptr.is_null() {
        return TgString {
            ptr: ptr::null(),
            len: 0,
        };
    }
    let start = start.min(s.len);
    let taken = len.min(s.len - start);
    TgString {
        ptr: unsafe { s.ptr.add(start as usize) },
        len: taken,
    }
}

#[cfg(test)]
mod tests {
    use super::{tg_string_substring, TgString};

    fn tg(s: &'static str) -> TgString {
        TgString {
            ptr: s.as_ptr().cast::<std::os::raw::c_char>(),
            len: s.len() as u64,
        }
    }

    fn read(s: TgString) -> String {
        if s.ptr.is_null() {
            return String::new();
        }
        let bytes = unsafe { std::slice::from_raw_parts(s.ptr.cast::<u8>(), s.len as usize) };
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// The distinguishing case: a start offset greater than the requested
    /// length. An end-index reading returns `""` here; ADR 20.8.26c is about
    /// exactly this input.
    #[test]
    fn start_beyond_the_length_still_takes_len_bytes() {
        assert_eq!(read(tg_string_substring(tg("abcdefghij"), 6, 3)), "ghi");
    }

    #[test]
    fn a_zero_start_takes_a_prefix() {
        assert_eq!(read(tg_string_substring(tg("abcdefghij"), 0, 4)), "abcd");
    }

    #[test]
    fn a_zero_length_is_empty_rather_than_the_rest() {
        assert_eq!(read(tg_string_substring(tg("abcdefghij"), 2, 0)), "");
    }

    #[test]
    fn an_overlong_length_is_clamped_to_what_remains() {
        assert_eq!(read(tg_string_substring(tg("abc"), 1, 99)), "bc");
    }

    #[test]
    fn a_start_past_the_end_is_empty_rather_than_out_of_bounds() {
        let out = tg_string_substring(tg("abc"), 99, 3);
        assert_eq!(out.len, 0);
        assert_eq!(read(out), "");
    }

    #[test]
    fn a_null_subject_yields_the_null_empty_string() {
        let out = tg_string_substring(
            TgString {
                ptr: std::ptr::null(),
                len: 7,
            },
            0,
            3,
        );
        assert!(out.ptr.is_null());
        assert_eq!(out.len, 0);
    }
}
