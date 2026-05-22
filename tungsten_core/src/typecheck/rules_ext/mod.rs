//! Extended typing rules: strings, arithmetic, booleans, refs, ADTs.
//!
//! These functions implement typing rules for extended term forms,
//! called from the main `type_of` match in `rules.rs`.

mod adt;
mod arithmetic;
mod control_flow;
mod recursion;
mod refs;
mod strings;

pub(super) use adt::{type_of_adt_construct, type_of_adt_match};
pub(super) use arithmetic::{
    type_of_bool_binop, type_of_bool_not, type_of_nat_binop, type_of_nat_cmp,
};
pub(super) use control_flow::type_of_return;
pub(super) use recursion::{type_of_fix, type_of_fold, type_of_subst, type_of_unfold};
pub(super) use refs::{type_of_ref_get, type_of_ref_set};
pub(super) use strings::{
    type_of_str_char_at, type_of_str_concat, type_of_str_eq, type_of_str_len, type_of_str_substring,
};
