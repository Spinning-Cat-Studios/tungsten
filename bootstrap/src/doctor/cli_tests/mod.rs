//! Tests: which `doctor check` spellings clap accepts, and what its defaults
//! resolve to.
//!
//! Split from [`super::tests`] when ADR 13.8.26c's review regrouped two
//! namespaces and pushed that file past its ceiling. The seam is real: these
//! assert on the *command surface* — paths, flags, defaults, hidden aliases —
//! while `tests.rs` asserts on what the doctor's checks compute.
//!
//! Split again into a directory by ADR 15.8.26a, for the same reason and along
//! the seams the three test modules already had.

mod aliases;
mod grouping;
mod name_collisions;
