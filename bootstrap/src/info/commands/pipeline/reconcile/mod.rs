//! Reading the *other* representation of each fact the inventory documents, so
//! `tests.rs` can compare the two (ADR 28.7.26f D2/D3).
//!
//! One module per source of truth the listing claims to describe:
//! - [`flags`] — the argument list clap accepts for a given command.
//! - [`make_targets`] — the rule headers `Makefile` and `make/*.mk` declare.
//!
//! Subcommand paths need no module here: `list_commands::leaf_paths` already
//! walks the clap tree for `tungsten commands`, and reusing it is what stops the
//! reconciliation and the shipped listing disagreeing about what exists.
//!
//! Each extractor is a pure function over text or over the clap tree, unit
//! tested in both directions — that a comment is *not* a make target matters as
//! much as that a rule header is, because a scan that silently over-accepts
//! makes the reconciliation permissive rather than failing it.

pub mod flags;
pub mod make_targets;
