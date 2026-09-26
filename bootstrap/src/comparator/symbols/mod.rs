//! The comparator symbol namespace.
//!
//! Two halves of one correspondence, kept together because neither is
//! meaningful alone:
//!
//! - [`mangling`] maps a concrete `Type` to the symbol its comparator is
//!   defined under;
//! - [`requests`] maps a symbol recorded at a `__compare` call site back to
//!   the `Type` it was requested at, so synthesis can resolve it later.
//!
//! The property that makes the pair work is that the *same* mangling is used
//! at the call site and during synthesis, so the two always agree. Note the
//! mapping is deliberately **not injective**: a `Type::Adt` mangles from its
//! name and type arguments alone, since the variants are determined by them —
//! a subtlety the closure walk has to respect (`discover::synth_closure_bounded`).

pub mod mangling;
pub mod requests;
