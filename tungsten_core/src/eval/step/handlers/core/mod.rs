//! Core type-theory step handlers.
//!
//! Handlers for application, let, if, natrec, natind, case, subst,
//! pair, fst, snd, tyapp, annot, and unfold.

mod app_let_if;
mod data_and_types;
mod nat_recursion;

pub(in crate::eval) use app_let_if::{step_app, step_if, step_let};
pub(in crate::eval) use data_and_types::{
    step_annot, step_case, step_fst, step_pair, step_snd, step_subst, step_tyapp, step_unfold,
};
pub(in crate::eval) use nat_recursion::{step_natind, step_natrec};
