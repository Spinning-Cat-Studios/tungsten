#![allow(unknown_lints)]
// Clippy lint policy: suppress pedantic/style lints at crate level.
// These are all cosmetic — no correctness issues. Tighten incrementally in v1.5.
#![allow(
    // --- Clippy: style & formatting ---
    clippy::uninlined_format_args,
    clippy::unreadable_literal,
    clippy::write_with_newline,
    clippy::let_and_return,
    clippy::bool_to_int_with_if,
    clippy::range_plus_one,
    // --- Clippy: documentation ---
    clippy::doc_markdown,
    clippy::doc_link_with_quotes,
    clippy::doc_comment_double_space_linebreaks,
    clippy::doc_lazy_continuation,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    // --- Clippy: complexity & refactoring ---
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::collapsible_if,
    clippy::single_match_else,
    clippy::redundant_else,
    clippy::needless_continue,
    clippy::if_not_else,
    clippy::unnecessary_map_or,
    clippy::manual_map,
    clippy::manual_strip,
    clippy::manual_let_else,
    clippy::unnested_or_patterns,
    // --- Clippy: must-use / API ---
    clippy::must_use_candidate,
    clippy::return_self_not_must_use,
    // --- Clippy: match arms ---
    clippy::match_same_arms,
    // --- Clippy: closures & conversions ---
    clippy::redundant_closure,
    clippy::redundant_closure_for_method_calls,
    clippy::useless_conversion,
    clippy::explicit_iter_loop,
    clippy::explicit_auto_deref,
    clippy::cloned_instead_of_copied,
    // --- Clippy: clone / copy ---
    clippy::implicit_clone,
    clippy::clone_on_copy,
    clippy::assigning_clones,
    // --- Clippy: error handling ---
    clippy::result_large_err,
    clippy::unnecessary_wraps,
    clippy::io_other_error,
    // --- Clippy: unused / dead ---
    clippy::unused_self,
    clippy::needless_pass_by_value,
    clippy::only_used_in_recursion,
    clippy::self_only_used_in_recursion, // Rust 1.93+ split from only_used_in_recursion
    // --- Clippy: casting ---
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    // --- Clippy: imports ---
    clippy::wildcard_imports,
    // --- Clippy: borrows / references ---
    clippy::needless_borrow,
    clippy::needless_borrows_for_generic_args,
    clippy::ptr_arg,
    // --- Clippy: lifetimes ---
    clippy::elidable_lifetime_names,
    // --- Clippy: derivation ---
    clippy::derivable_impls,
    // --- Clippy: enum ---
    clippy::large_enum_variant,
    // --- Clippy: hasher ---
    clippy::implicit_hasher,
    // --- Clippy: option / map ---
    clippy::map_unwrap_or,
    clippy::option_map_unit_fn,
    // --- Clippy: misc ---
    clippy::missing_const_for_thread_local,
    clippy::format_collect,
    clippy::unnecessary_debug_formatting,
    // --- Rustc warnings ---
    unused_imports,
    unused_variables,
    dead_code,
)]

//! Bootstrap compiler for the Tungsten proof language.
//!
//! This crate provides lexing, parsing, and elaboration
//! for Tungsten source code. It produces surface AST that can be
//! elaborated into core terms for type checking via `tungsten_core`.
//!
//! # Architecture
//!
//! ```text
//! .tg source
//!     │
//!     ▼
//! ┌─────────┐
//! │  Lexer  │  → Vec<Token>
//! └─────────┘
//!     │
//!     ▼
//! ┌─────────┐
//! │ Parser  │  → SourceFile (AST)
//! └─────────┘
//!     │
//!     ▼
//! ┌────────────┐
//! │ Elaborator │ → tungsten_core::Term
//! └────────────┘
//!     │
//!     ▼
//! ┌─────────────┐
//! │ Type Check  │  (tungsten_core)
//! └─────────────┘
//! ```
//!
//! # Example
//!
//! ```
//! use tungsten_bootstrap::{parse, elaborate};
//! use tungsten_core::Context;
//!
//! let source = r#"
//!     fn add(x: Nat, y: Nat) -> Nat {
//!         x + y
//!     }
//! "#;
//!
//! // Parse the source
//! let (ast, parse_errors) = parse(source);
//!
//! if parse_errors.is_empty() {
//!     // Elaborate to Core
//!     let mut ctx = Context::new();
//!     match elaborate(&ast, &mut ctx) {
//!         Ok(defs) => println!("Elaborated {} definitions", defs.len()),
//!         Err(errors) => {
//!             for err in errors {
//!                 eprintln!("Error: {}", err);
//!             }
//!         }
//!     }
//! }
//! ```

pub mod ast;
pub mod cache;
pub mod config;
pub mod driver;
pub mod elaborate;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod token;
pub mod utils;

// Re-exports for convenience
pub use ast::{
    AxiomDef, BinOp, Expr, Field, FunctionDef, Ident, Item, LambdaParam, LiteralPattern, MatchArm,
    Param, Pattern, Sorry, SourceFile, Stmt, TheoremDef, TypeAlias, TypeDef, TypeExpr, TypeParam,
    UnaryOp, Variant,
};
pub use cache::BuildCache;
pub use elaborate::{
    elaborate, elaborate_with_warnings, CoreDef, ElabError, ElabErrorKind, ElabOutput, Elaborator,
    Env,
};
pub use error::{
    Diagnostic, DiagnosticRenderer, Label, LexError, ParseError, Severity, Suggestion,
};
pub use lexer::Lexer;
pub use parser::{parse, Parser};
pub use span::{LineIndex, Location, Span, Spanned};
pub use token::{keyword_from_str, Token, TokenKind};
