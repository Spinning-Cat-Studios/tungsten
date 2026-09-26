//! The error catalogue's **prose** side: what `explain error`'s descriptions
//! claim about the listing, checked against what the listing withholds
//! (ADR 19.8.26b).
//!
//! The catalogue describes itself twice. The data side — `CATEGORIES` and the
//! `code()` arms — is checked by [`super::catalogue`]. The prose side was read
//! by nothing at all, and ADR 15.8.26b left five claims of exhaustiveness
//! standing behind a listing that had just stopped being exhaustive, with every
//! data-side test green.
//!
//! Two layers, because the judgement and the binding fail in different ways:
//!
//! - [`predicate`] decides, over injected strings, whether a description is
//!   consistent with a withheld set. All four cases live there, and
//!   [`predicate_tests`] asserts them.
//! - [`surfaces`] renders the real help, reads the real message constants, and
//!   feeds them the real `UNLISTED_KINDS`.

mod predicate;
mod predicate_tests;
mod surfaces;
