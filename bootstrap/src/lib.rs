// Clippy lint policy — see ADR 18.5.26h for triage decisions.
#![allow(unknown_lints)]
// Reason: devcontainer (1.95) and host (1.91) have different lint names
// --- Keep: docs deferred to v2.0 ---
#![allow(clippy::doc_markdown)] // Reason: docs deferred to v2.0 (369 instances)
#![allow(clippy::doc_link_with_quotes)] // Reason: docs deferred to v2.0
#![allow(clippy::doc_comment_double_space_linebreaks)] // Reason: docs deferred to v2.0
#![allow(clippy::doc_lazy_continuation)] // Reason: docs deferred to v2.0
#![allow(clippy::missing_errors_doc)] // Reason: docs deferred to v2.0 (53 instances)
#![allow(clippy::missing_panics_doc)] // Reason: docs deferred to v2.0
// --- Keep: permanent ---
#![allow(clippy::result_large_err)] // Reason: TypeError is intentionally large (236 instances)
#![allow(clippy::too_many_lines)] // Reason: governed by project check-complexity tooling
#![allow(clippy::too_many_arguments)] // Reason: compiler pipeline functions take many context params; refactoring out of scope
#![allow(clippy::type_complexity)] // Reason: elaboration types are inherently complex
#![allow(clippy::large_enum_variant)]
// Reason: AST/CIR enum variants vary widely; boxing smallest hurts ergonomics
// --- Keep: style preference (widespread, fixing adds noise) ---
#![allow(clippy::must_use_candidate)] // Reason: 134 instances; adding #[must_use] everywhere changes public API
#![allow(clippy::return_self_not_must_use)] // Reason: builder-pattern methods; changes public API
#![allow(clippy::match_same_arms)] // Reason: intentionally separate arms for clarity (42 instances)
#![allow(clippy::manual_let_else)] // Reason: 59 instances; existing match-based let is idiomatic
#![allow(clippy::needless_pass_by_value)] // Reason: 13 instances; changing to &T would ripple through call sites
#![allow(clippy::unused_self)] // Reason: 35 instances; methods use &self for API consistency
#![allow(clippy::only_used_in_recursion)]
// Reason: recursive helper params are intentional
// --- Keep: style preference (widespread, auto-fix incomplete) ---
#![allow(clippy::uninlined_format_args)] // Reason: 159 instances; purely cosmetic, partial auto-fix
#![allow(clippy::redundant_closure)] // Reason: auto-fix applied most; remainder are method calls
#![allow(clippy::redundant_closure_for_method_calls)] // Reason: 38 instances; cosmetic
#![allow(clippy::needless_lifetimes)] // Reason: auto-fix applied most
#![allow(clippy::elidable_lifetime_names)] // Reason: 74 instances; cosmetic
#![allow(clippy::ptr_arg)] // Reason: 15 instances; changing &PathBuf→&Path ripples through call sites
#![allow(clippy::wildcard_imports)] // Reason: 13 instances; glob imports used for prelude-style modules
#![allow(clippy::clone_on_copy)] // Reason: 12 instances; partial auto-fix applied
#![allow(clippy::write_with_newline)] // Reason: explicit write! + \n preferred in codegen output
#![allow(clippy::unnecessary_wraps)] // Reason: 10+6 instances; Result/Option wrappers for API consistency
#![allow(clippy::similar_names)] // Reason: compiler variables naturally have similar names
#![allow(clippy::stable_sort_primitive)] // Reason: 14 instances; sort() vs sort_unstable() perf is negligible here
#![allow(clippy::format_push_string)] // Reason: 13 instances; format! + push_str used for readability
#![allow(clippy::single_match_else)] // Reason: 12 instances; explicit match preferred for clarity
#![allow(clippy::map_unwrap_or)] // Reason: 12 instances; map().unwrap_or() reads better than map_or()
#![allow(clippy::if_not_else)] // Reason: 10 instances; negated conditions sometimes read better
#![allow(clippy::unnecessary_map_or)] // Reason: 10 instances; map_or used for readability
#![allow(clippy::implicit_clone)] // Reason: 8 instances; .to_string() on &String is idiomatic
#![allow(clippy::format_collect)] // Reason: 7 instances; format!+collect pattern used in codegen
#![allow(clippy::struct_excessive_bools)] // Reason: config/options structs naturally have many bool fields
#![allow(clippy::option_map_unit_fn)] // Reason: map() for side effects is sometimes clearer
#![allow(clippy::ref_option)] // Reason: &Option<T> used at API boundaries
#![allow(clippy::collapsible_if)] // Reason: separate if blocks preferred for clarity
#![allow(clippy::match_wildcard_for_single_variants)] // Reason: _ catch-all is intentional for future variants
// --- Keep: FFI / casting ---
#![allow(clippy::cast_possible_truncation)] // Reason: handle/index casts checked at call sites
#![allow(clippy::cast_precision_loss)] // Reason: intentional f64 conversions for stats/profiling
#![allow(clippy::borrow_as_ptr)]
// Reason: FFI code using &x as *const _ is idiomatic
// --- Keep: rustc warnings (in-progress code) ---
#![allow(unused_imports)] // Reason: modules under active development
#![allow(unused_variables)] // Reason: modules under active development
#![allow(dead_code)] // Reason: modules under active development

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
pub mod doctor;
pub mod driver;
pub mod elaborate;
pub mod error;
pub mod fold_analysis;
pub mod lexer;
pub mod parser;
pub mod sidecar;
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
    elaborate, elaborate_with_warnings, elaborate_with_warnings_full, AdtOrigin, CoreDef,
    ElabError, ElabErrorKind, ElabOutput, Elaborator, Env, Note, TraceFrame, TypeProvenance,
};
pub use error::{
    Diagnostic, DiagnosticRenderer, Label, LexError, ParseError, Severity, Suggestion,
};
pub use lexer::Lexer;
pub use parser::{parse, Parser};
pub use span::{LineIndex, Location, Span, Spanned};
pub use token::{keyword_from_str, Token, TokenKind};
