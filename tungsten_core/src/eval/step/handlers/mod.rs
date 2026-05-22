//! Handler functions for step() match arms.
//!
//! Split into submodules by domain:
//! - `core`: Core type theory (app, let, if, natrec, pairs, case, etc.)
//! - `extensions`: Strings, booleans, and ADT operations.

mod core;
mod extensions;

pub(in crate::eval) use self::core::{
    step_annot, step_app, step_case, step_fst, step_if, step_let, step_natind, step_natrec,
    step_pair, step_snd, step_subst, step_tyapp, step_unfold,
};
pub(in crate::eval) use extensions::{
    step_adt_match, step_bool_not, step_str_char_at, step_str_concat, step_str_eq, step_str_len,
    step_str_substring,
};
