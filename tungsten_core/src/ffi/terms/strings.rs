//! String term constructors for FFI (Phase 3C).
//!
//! Split out of `primitives` by ADR 20.8.26c, when `tg_term_str_substring`
//! took that file past its function-count budget. The seam is the one the
//! reader already has: `substring`'s third argument is a **length**, and the
//! other three constructors here are its neighbours in the same surface — the
//! set the self-hosted elaborator's interception table has to be able to build.
//!
//! All constructors are O(1) node pushes (ADR 2.7.26a §4).

use super::nodes::TermNode;
use super::valid_terms;
use crate::ffi::{with_arena, TermHandle, INVALID_HANDLE};

/// Construct string concatenation: a ++ b
#[no_mangle]
pub extern "C" fn tg_term_str_concat(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_string_term(a, b, TermNode::StrConcat)
}

/// Construct string equality: a == b
#[no_mangle]
pub extern "C" fn tg_term_str_eq(a: TermHandle, b: TermHandle) -> TermHandle {
    binary_string_term(a, b, TermNode::StrEq)
}

/// Construct string length: strlen s
#[no_mangle]
pub extern "C" fn tg_term_str_len(s: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[s]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::StrLen(s))
    })
}

/// Construct a substring: `substring s start len`.
///
/// The third argument is a **length**, not an end index (ADR 20.8.26c). The
/// self-hosted elaborator needs this constructor because it intercepts
/// `substring` before name resolution exactly as the bootstrap does; without it,
/// the same call resolves to whichever `.tg` function happens to carry the name,
/// which is how the two compilers came to disagree about that argument.
#[no_mangle]
pub extern "C" fn tg_term_str_substring(
    s: TermHandle,
    start: TermHandle,
    len: TermHandle,
) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[s, start, len]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::StrSubstring(s, start, len))
    })
}

/// Shared O(1) binary-operation constructor.
///
/// A local copy of `primitives`' helper rather than a shared `pub(super)` one:
/// it is four lines, and exporting it would put a third name in a module surface
/// this split exists to keep small.
fn binary_string_term(
    a: TermHandle,
    b: TermHandle,
    make: fn(TermHandle, TermHandle) -> TermNode,
) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[a, b]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(make(a, b))
    })
}

#[cfg(test)]
pub(in crate::ffi) mod tests {
    use super::{tg_term_str_concat, tg_term_str_eq, tg_term_str_len, tg_term_str_substring};
    use crate::ffi::TermHandle;
    use crate::terms::Term;

    /// (constructed handle, expected owned term) for every string constructor.
    ///
    /// Shared with `ffi::term_surface_tests`, which materializes each handle and
    /// compares — the cases moved here with the functions (ADR 20.8.26c) rather
    /// than being duplicated, so there is still exactly one list of what this
    /// module builds.
    pub(in crate::ffi) fn string_cases(
        zero: TermHandle,
        one: TermHandle,
        tru: TermHandle,
    ) -> Vec<(TermHandle, Term)> {
        let bx = Box::new;
        vec![
            (
                tg_term_str_concat(zero, one),
                Term::StrConcat(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (
                tg_term_str_eq(zero, one),
                Term::StrEq(bx(Term::Zero), bx(Term::NatLit(1))),
            ),
            (tg_term_str_len(zero), Term::StrLen(bx(Term::Zero))),
            (
                tg_term_str_substring(zero, one, tru),
                Term::StrSubstring(bx(Term::Zero), bx(Term::NatLit(1)), bx(Term::True)),
            ),
        ]
    }
}
