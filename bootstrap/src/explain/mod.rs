//! `tungsten explain` — pedagogical CLI namespace for understanding errors and types.
//!
//! Unlike `tungsten info` (which shows *what* exists), `explain` answers *why* and *how*:
//! - `explain error <kind>` — detailed error explanations with examples
//! - `explain type <string>` — step-by-step structural type decoding
//!
//! All explain commands are fully static — no file I/O or elaboration required.
//! See ADR 14.4.26a for design rationale.

mod error_catalogue;
mod explanations;
pub(crate) mod l2_error_catalogue;
mod type_explainer;
mod type_parser;

#[cfg(test)]
mod tests;

use std::process::ExitCode;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum ExplainCommands {
    /// Explain an elaboration error kind
    ///
    /// With no argument, lists all error kinds grouped by category.
    /// With an error kind name, prints a detailed explanation with examples.
    /// Use --l2 for self-hosted compiler (L2) error codes.
    ///
    /// Examples:
    ///   tungsten explain error
    ///   tungsten explain error `TypeMismatch`
    ///   tungsten explain error `UndefinedVariable`
    ///   tungsten explain error --l2
    ///   tungsten explain error --l2 E0001
    ///   tungsten explain error --l2 `ErrTypeMismatch`
    Error {
        /// Error kind name (e.g., "`TypeMismatch`"). Omit to list all.
        kind: Option<String>,

        /// Show L2 (self-hosted compiler) error codes instead of L1
        #[arg(long)]
        l2: bool,
    },

    /// Decode a structural Core IR type step by step
    ///
    /// Parses the canonical Display output of a Core IR type and explains
    /// each construct (μ, ∀, ×, +, →) in plain language.
    ///
    /// Examples:
    ///   tungsten explain type "Nat"
    ///   tungsten explain type "Nat → Bool"
    ///   tungsten explain type "`μα_List`. (Unit + (Nat × `α_List`))"
    #[command(name = "type")]
    Type {
        /// Structural type string (from error messages or --dump-ir output)
        type_string: String,
    },

    /// Explain recursion classification categories
    ///
    /// Documents the four recursion types detected by `tungsten doctor audit-recursion`:
    /// tail-recursive, tree-recursive, linear non-tail, and general/unbounded.
    ///
    /// Examples:
    ///   tungsten explain recursion-types
    RecursionTypes,

    /// Understand stack overflow crashes in Tungsten programs
    ///
    /// Explains common causes, diagnostic tools, and recovery strategies.
    ///
    /// Examples:
    ///   tungsten explain stack-overflow
    StackOverflow,

    /// Understand mutual type recursion and its impact on encoding
    ///
    /// Explains how mutually recursive types affect μ-type encoding
    /// and what tools are available for diagnosis.
    ///
    /// Examples:
    ///   tungsten explain mutual-recursion
    MutualRecursion,
}

/// Dispatch an explain subcommand.
pub fn cmd_explain(cmd: ExplainCommands) -> ExitCode {
    match cmd {
        ExplainCommands::Error {
            kind: None,
            l2: true,
        } => {
            l2_error_catalogue::print_l2_error_list();
            ExitCode::SUCCESS
        }
        ExplainCommands::Error {
            kind: Some(name),
            l2: true,
        } => l2_error_catalogue::print_l2_error_explanation(&name),
        ExplainCommands::Error {
            kind: None,
            l2: false,
        } => {
            error_catalogue::print_error_list();
            ExitCode::SUCCESS
        }
        ExplainCommands::Error {
            kind: Some(name),
            l2: false,
        } => error_catalogue::print_error_explanation(&name),
        ExplainCommands::Type { type_string } => explain_type_string(&type_string),
        ExplainCommands::RecursionTypes => {
            print_recursion_types_explanation();
            ExitCode::SUCCESS
        }
        ExplainCommands::StackOverflow => {
            print_stack_overflow_explanation();
            ExitCode::SUCCESS
        }
        ExplainCommands::MutualRecursion => {
            print_mutual_recursion_explanation();
            ExitCode::SUCCESS
        }
    }
}

fn explain_type_string(type_string: &str) -> ExitCode {
    match type_parser::parse_type(type_string) {
        Ok(ty) => {
            type_explainer::explain_type(&ty);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: failed to parse type string: {e}");
            eprintln!();
            eprintln!("Expected canonical Display format using Unicode operators:");
            eprintln!("  → (arrow)  × (product)  + (sum)  μ (mu)  ∀ (forall)");
            eprintln!();
            eprintln!("Example: tungsten explain type \"μα_List. (Unit + (Nat × α_List))\"");
            ExitCode::FAILURE
        }
    }
}

fn print_recursion_types_explanation() {
    println!(
        "\
Tungsten classifies recursive functions into four categories:

  TAIL-RECURSIVE
    The recursive call is the last operation — no work after it returns.
    These are eligible for musttail optimization (constant stack).
    Stack depth: O(1) — the call reuses the current stack frame.
    Example: filter_trivia_acc, list_reverse

  TREE-RECURSIVE
    The function makes multiple recursive calls per invocation.
    Stack depth is O(tree height), not O(input size).
    Safe for balanced structures; risky for degenerate inputs.
    Example: elab_type (recurses into sub-expressions)

  LINEAR NON-TAIL
    One recursive call per branch, but not in tail position.
    There is work to do after the recursive call returns.
    Stack depth is O(n). May overflow on large linear inputs.
    Example: map (builds result after recursive return)

  GENERAL / UNBOUNDED
    Recursion pattern does not fit the above categories,
    or involves indirect calls through higher-order functions.
    Requires manual analysis for stack safety.

Diagnostic tools:
  tungsten doctor audit-recursion <file>
    Identify all recursive functions and their classification

  tungsten explain stack-overflow
    Understanding stack overflow crashes"
    );
}

fn print_stack_overflow_explanation() {
    println!(
        "\
STACK OVERFLOW IN TUNGSTEN PROGRAMS

A stack overflow occurs when a function's call chain exceeds the
available stack space (typically 8 MB on macOS, 2-8 MB on Linux).

Common causes in Tungsten:
  1. Tail-recursive function without musttail optimization
     → Fixed by the compiler's musttail pass (18.4.26d)

  2. Tree-recursive function on deeply nested input
     → Stack depth is O(tree height), not O(input size)
     → A future depth guard may allow setting a recursion limit

  3. Mutual recursion without a base case
     → The functions call each other indefinitely

  4. Linear non-tail recursion on large input
     → Each recursive call adds a frame to the stack

Diagnostic tools:
  tungsten doctor audit-recursion <file>
    Identify all recursive functions and their classification

  tungsten compile --named-lambdas <file>
    Use source names in IR for readable backtraces

  tungsten info symbols <file>
    Query the lambda → source name mapping

Runtime behavior:
  When a stack overflow occurs, the compiled program's signal handler
  (if installed) will print a diagnostic message identifying the crash
  as a stack overflow, rather than showing a raw SIGSEGV.

  Set TUNGSTEN_NO_SIGNAL_HANDLER=1 to disable the handler (e.g.,
  when running under a debugger)."
    );
}

fn print_mutual_recursion_explanation() {
    println!(
        "\
MUTUAL TYPE RECURSION IN TUNGSTEN

Two or more types are mutually recursive when they reference each
other in their constructors:

  type TypeExpr =
    | TyEq(Expr, Expr, Span)     ← references Expr
    ...

  type Expr =
    | ExprAnnot(Expr, TypeExpr, Span)  ← references TypeExpr
    ...

Impact on type encoding:
  The μ-type encoding uses a single μ-binder per type:
    Mu(α_Expr, Sum([...]))

  For self-recursive types, this works correctly: the type's own
  self-references are replaced with the μ-variable (α_Expr).

  For mutually recursive types, cross-references produce a mix of
  μ-variables and bare TyVars. The type checker may not be able to
  reconcile these different representations, leading to type errors
  or TyVar escapes.

Cycle detection:
  During encoding, the compiler tracks an encoding stack. When it
  encounters a type already on the stack, it emits a TyVar reference
  instead of infinitely recursing. This cycle break is correct but
  produces structurally different types depending on encoding order.

Diagnostic tools:
  tungsten doctor audit-mutual-types <file>
    Identify all mutually recursive type groups using SCC analysis

  tungsten compile --trace-encoding[=TypeName] <file>
    Trace the encoding stack, cycle detection, and μ-variable
    assignments during compilation

  tungsten info encoding <TypeName> <file>
    Show the encoding strategy for a specific ADT"
    );
}
