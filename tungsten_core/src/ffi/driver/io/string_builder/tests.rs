//! Tests for the `StringBuilder` runtime (ADR 14.9.26a).
//!
//! The abort paths are not exercised in-process — `abort()` would kill the
//! harness — so what is pinned is the pure classifier every entry point
//! decides through, the growth policy, the scalar encoder, and the text of
//! the lines written before an abort (AC 3, AC 7).

use std::ffi::c_char;
use std::ptr;

use super::{
    dead_handle_message, encode_scalar, grow_policy, growth_recorded, header_state,
    invalid_scalar_message, tg_string_builder_len, tg_string_builder_new,
    tg_string_builder_push_char, tg_string_builder_push_str, tg_string_builder_to_string,
    tg_string_builder_with_capacity, HeaderState, TgString, TgStringBuilder, CONSUMED, LIVE,
    MIN_CAPACITY,
};

/// Borrow a `&str` as the by-value `TgString` the C ABI passes.
fn tg(s: &str) -> TgString {
    TgString {
        ptr: s.as_ptr().cast::<c_char>(),
        len: s.len() as u64,
    }
}

/// Copy a `TgString`'s bytes out; the null empty string reads as `""`.
///
/// The evaluator bridge's reader, not a copy of it: the same function the
/// arms use to read `to_string`'s result is what these tests read with.
fn read(s: TgString) -> String {
    crate::ffi::evaluator_bridges::tg_string_text(s)
}

/// Read the header a live handle points at.
fn header_of(handle: u64) -> TgStringBuilder {
    unsafe { *(handle as *const TgStringBuilder) }
}

fn live_header() -> TgStringBuilder {
    TgStringBuilder {
        ptr: ptr::null_mut(),
        len: 0,
        cap: 0,
        state: LIVE,
    }
}

// ---------------------------------------------------------------------------
// header_state — the one decision procedure (AC 3)
// ---------------------------------------------------------------------------

/// 14.9.26a AC3: consumption is final — the classifier is the entry points'
/// only decision procedure, so its three answers are pinned one by one.
#[test]
fn a_live_header_is_live() {
    assert_eq!(header_state(0x1000, Some(live_header())), HeaderState::Live);
}

#[test]
fn a_consumed_header_is_consumed() {
    let consumed = TgStringBuilder {
        state: CONSUMED,
        ..live_header()
    };
    assert_eq!(header_state(0x1000, Some(consumed)), HeaderState::Consumed);
}

/// Any word that is not `LIVE` reads as consumed — a reused header carries
/// arbitrary bits and refusing it is the safe reading.
#[test]
fn an_unrecognised_state_word_is_consumed_not_live() {
    let garbage = TgStringBuilder {
        state: 0xDEAD,
        ..live_header()
    };
    assert_eq!(header_state(0x1000, Some(garbage)), HeaderState::Consumed);
}

#[test]
fn no_header_is_null() {
    assert_eq!(header_state(0, None), HeaderState::Null);
}

/// A zero handle is null even if a caller fabricated a header for it — the
/// classifier does not trust the header over the address.
#[test]
fn a_zero_handle_is_null_whatever_header_is_offered() {
    assert_eq!(header_state(0, Some(live_header())), HeaderState::Null);
}

// ---------------------------------------------------------------------------
// The abort lines (AC 3) — asserted as strings, never fired
// ---------------------------------------------------------------------------

#[test]
fn the_dead_handle_line_names_the_entry_point_the_handle_and_the_reason() {
    let line = dead_handle_message("tg_string_builder_len", 0x1000, HeaderState::Consumed);
    assert_eq!(
        line,
        "tungsten: tg_string_builder_len: StringBuilder handle 0x1000 is \
         already consumed by to_string; aborting"
    );
}

#[test]
fn the_dead_handle_line_for_a_null_handle_says_null() {
    let line = dead_handle_message("tg_string_builder_push_str", 0, HeaderState::Null);
    assert_eq!(
        line,
        "tungsten: tg_string_builder_push_str: StringBuilder handle 0x0 is null; aborting"
    );
}

#[test]
fn the_invalid_scalar_line_names_the_value_and_the_builder() {
    assert_eq!(
        invalid_scalar_message(0x1000, 0xD800),
        "tungsten: tg_string_builder_push_char: 0xd800 is not a Unicode scalar value \
         (StringBuilder handle 0x1000); aborting"
    );
}

// ---------------------------------------------------------------------------
// grow_policy (AC 7)
// ---------------------------------------------------------------------------

/// The named case: the first push from `cap = 0` is `max(n, MIN_CAPACITY)`,
/// not a doubling of zero.
#[test]
fn the_first_push_from_zero_allocates_at_least_the_minimum() {
    assert_eq!(grow_policy(0, 0, 1), MIN_CAPACITY);
    assert_eq!(grow_policy(0, 0, MIN_CAPACITY), MIN_CAPACITY);
    assert_eq!(grow_policy(0, 0, MIN_CAPACITY + 1), MIN_CAPACITY + 1);
}

#[test]
fn a_small_push_into_a_full_buffer_doubles() {
    assert_eq!(grow_policy(32, 32, 1), 64);
}

/// A push larger than a doubling would cover grows to what it needs.
#[test]
fn a_large_push_grows_to_what_it_needs_not_merely_double() {
    assert_eq!(grow_policy(32, 32, 100), 132);
}

/// The boundary between the two arms: exactly double is exactly double.
#[test]
fn a_push_that_needs_exactly_double_takes_double() {
    assert_eq!(grow_policy(32, 32, 32), 64);
}

/// `len` rather than `cap` is what `add` is measured against.
#[test]
fn need_is_measured_from_len_not_cap() {
    assert_eq!(grow_policy(10, 64, 200), 210);
}

/// The profiler is told the growth, not the new capacity — realloc reuses
/// the old buffer, so counting it again would double the string class.
#[test]
fn the_profiler_is_told_the_growth_not_the_new_capacity() {
    assert_eq!(growth_recorded(16, 32), 16);
    assert_eq!(growth_recorded(0, 16), 16);
    assert_eq!(growth_recorded(32, 32), 0);
}

// ---------------------------------------------------------------------------
// encode_scalar (push_char's decision half)
// ---------------------------------------------------------------------------

#[test]
fn ascii_encodes_to_one_byte() {
    assert_eq!(encode_scalar(0x41), Some(([0x41, 0, 0, 0], 1)));
}

#[test]
fn a_multi_byte_scalar_encodes_to_its_utf8() {
    // U+00E9 é → C3 A9; U+1F600 😀 → F0 9F 98 80
    assert_eq!(encode_scalar(0xE9), Some(([0xC3, 0xA9, 0, 0], 2)));
    assert_eq!(encode_scalar(0x1F600), Some(([0xF0, 0x9F, 0x98, 0x80], 4)));
}

#[test]
fn a_surrogate_is_refused() {
    assert_eq!(encode_scalar(0xD800), None);
    assert_eq!(encode_scalar(0xDFFF), None);
}

#[test]
fn a_value_above_the_last_scalar_is_refused() {
    assert_eq!(encode_scalar(0x110000), None);
    assert_eq!(encode_scalar(u64::MAX), None);
}

// ---------------------------------------------------------------------------
// The entry points, on the happy path (AC 2)
// ---------------------------------------------------------------------------

#[test]
fn a_new_builder_is_empty_and_live() {
    let sb = tg_string_builder_new();
    assert_ne!(sb, 0);
    let header = header_of(sb);
    assert!(header.ptr.is_null());
    assert_eq!((header.len, header.cap, header.state), (0, 0, LIVE));
    assert_eq!(tg_string_builder_len(sb), 0);
}

#[test]
fn an_empty_builder_yields_the_null_empty_string() {
    let out = tg_string_builder_to_string(tg_string_builder_new());
    assert!(out.ptr.is_null());
    assert_eq!(out.len, 0);
}

#[test]
fn pushes_accumulate_in_order() {
    let sb = tg_string_builder_new();
    let same = tg_string_builder_push_str(sb, tg("hello"));
    assert_eq!(same, sb, "push_str returns the same handle");
    tg_string_builder_push_str(sb, tg(" "));
    tg_string_builder_push_str(sb, tg("world"));
    assert_eq!(tg_string_builder_len(sb), 11);
    assert_eq!(read(tg_string_builder_to_string(sb)), "hello world");
}

#[test]
fn a_null_or_empty_push_is_a_no_op() {
    let sb = tg_string_builder_new();
    tg_string_builder_push_str(sb, tg("a"));
    tg_string_builder_push_str(
        sb,
        TgString {
            ptr: ptr::null(),
            len: 3,
        },
    );
    tg_string_builder_push_str(sb, tg(""));
    assert_eq!(tg_string_builder_len(sb), 1);
}

#[test]
fn push_char_appends_utf8_and_len_counts_bytes() {
    let sb = tg_string_builder_new();
    tg_string_builder_push_char(sb, 0x41);
    tg_string_builder_push_char(sb, 0xE9);
    tg_string_builder_push_char(sb, 0x1F600);
    assert_eq!(tg_string_builder_len(sb), 1 + 2 + 4);
    assert_eq!(read(tg_string_builder_to_string(sb)), "Aé😀");
}

/// The buffer grows through the policy and the contents survive every
/// realloc: 10 000 pushes, compared against the equivalent Rust fold.
#[test]
fn ten_thousand_pushes_equal_the_fold() {
    let piece = "the quick brown fox jumps over the lazy dog!";
    let sb = tg_string_builder_new();
    for _ in 0..10_000 {
        tg_string_builder_push_str(sb, tg(piece));
    }
    assert_eq!(tg_string_builder_len(sb), piece.len() as u64 * 10_000);
    let header = header_of(sb);
    assert!(header.cap >= header.len);
    assert_eq!(read(tg_string_builder_to_string(sb)), piece.repeat(10_000));
}

/// `with_capacity(n)` then `n` bytes never reallocates: the buffer pointer is
/// the one allocated up front.
#[test]
fn with_capacity_then_that_many_bytes_does_not_regrow() {
    let sb = tg_string_builder_with_capacity(8);
    let before = header_of(sb);
    assert_eq!(before.cap, 8);
    assert!(!before.ptr.is_null());
    tg_string_builder_push_str(sb, tg("12345678"));
    let after = header_of(sb);
    assert_eq!(after.ptr, before.ptr);
    assert_eq!((after.len, after.cap), (8, 8));
    assert_eq!(read(tg_string_builder_to_string(sb)), "12345678");
}

// ---------------------------------------------------------------------------
// Consumption is final (AC 3): the header after to_string
// ---------------------------------------------------------------------------

#[test]
fn to_string_poisons_the_header_and_hands_out_the_buffer_itself() {
    let sb = tg_string_builder_new();
    tg_string_builder_push_str(sb, tg("abc"));
    let buffer = header_of(sb).ptr;
    let out = tg_string_builder_to_string(sb);
    assert_eq!(out.ptr.cast_mut().cast::<u8>(), buffer, "no copy");
    assert_eq!(read(out), "abc");
    let header = header_of(sb);
    assert!(header.ptr.is_null());
    assert_eq!((header.len, header.cap, header.state), (0, 0, CONSUMED));
    assert_eq!(header_state(sb, Some(header)), HeaderState::Consumed);
}
