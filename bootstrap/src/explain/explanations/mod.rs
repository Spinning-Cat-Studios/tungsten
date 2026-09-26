//! Per-category explanation data for error kinds.
//!
//! Each submodule handles one category of errors from the `get_explanation()` dispatch.
//!
//! - `name_resolution`: `UndefinedVariable`, `UndefinedType`, etc.
//! - `type_errors`: `TypeMismatch`, `CannotInferType`, etc.
//! - `control_flow`: `DeadCodeAfterReturn`, Try*, `LetElse`*
//! - `termination`: `CannotProveTermination`, `PartialInProof`
//! - `type_definitions`: `RecursiveAlias`, `NonStrictlyPositive`,
//!   `NestedRecursiveFamily`
//! - Remaining categories (phase 1, pattern matching, named records, entry
//!   point) are small enough to stay inline here.

mod control_flow;
mod equality;
mod name_resolution;
mod termination;
mod type_definitions;
mod type_errors;

use super::error_catalogue::ErrorExplanation;

/// Dispatch to the correct category function.
pub(super) fn get_explanation(name: &str) -> Option<ErrorExplanation> {
    name_resolution::name_resolution(name)
        .or_else(|| type_errors::type_errors(name))
        .or_else(|| phase1_restrictions(name))
        .or_else(|| pattern_matching(name))
        .or_else(|| control_flow::control_flow(name))
        .or_else(|| type_definitions::type_definitions(name))
        .or_else(|| equality::equality_proofs(name))
        .or_else(|| termination::termination(name))
        .or_else(|| named_record(name))
        .or_else(|| entry_point(name))
        .or_else(|| internal_errors(name))
}

/// `InternalError` (E9998) — resolvable so a user who hits one can look it
/// up, but deliberately absent from the no-argument listing: no program edit
/// can cause or fix one, so advertising the code would be noise
/// (ADR 15.8.26b).
fn internal_errors(name: &str) -> Option<ErrorExplanation> {
    (name == "InternalError").then_some(ErrorExplanation {
        name: "InternalError",
        code: "E9998",
        category: "Internal (not user-actionable)",
        summary: "the compiler broke its own invariant",
        detail: "\
The compiler detected that one of its own internal invariants was broken. \
This is a bug in the Tungsten compiler, not in your program — no change to \
your source code can legitimately cause or fix it.\n\
\n\
Please report it, including the full error message and, if possible, the \
source file that triggered it.",
        example: "\
// No source example: user input cannot legitimately trigger this code.",
        see_also: &[],
    })
}

fn phase1_restrictions(name: &str) -> Option<ErrorExplanation> {
    let exp = match name {
        "UnsupportedFeature" => ErrorExplanation {
            name: "UnsupportedFeature",
            code: "E0100",
            category: "Phase 1 Restrictions",
            summary: "feature not yet supported",
            detail: "\
This language feature exists in the grammar but is not yet implemented \
in the current phase of the compiler.\n\
\n\
Phase 1 focuses on core functionality: functions, ADTs, pattern matching, \
and dependent types. Some features are deferred to later phases.",
            example: "\
// Various features may trigger this depending on compiler version",
            see_also: &["MutabilityNotSupported"],
        },

        "MutabilityNotSupported" => ErrorExplanation {
            name: "MutabilityNotSupported",
            code: "E0102",
            category: "Phase 1 Restrictions",
            summary: "mutable bindings not supported",
            detail: "\
Tungsten does not support mutable variables (`let mut`). Use shadowing \
or functional patterns instead.\n\
\n\
This is intentional — immutability enables dependent type soundness.",
            example: "\
fn main() -> Nat {\n\
    let mut x = 1;    // error: mutable bindings not supported\n\
    x = 2;\n\
    x\n\
}\n\
\n\
// Fix: use shadowing\n\
fn main() -> Nat {\n\
    let x = 1;\n\
    let x = 2;    // shadows the previous x\n\
    x\n\
}",
            see_also: &["UnsupportedFeature"],
        },

        _ => return None,
    };
    Some(exp)
}

fn pattern_matching(name: &str) -> Option<ErrorExplanation> {
    let exp = match name {
        "NonExhaustiveMatch" => ErrorExplanation {
            name: "NonExhaustiveMatch",
            code: "E0020",
            category: "Pattern Matching",
            summary: "match does not cover all cases",
            detail: "\
A match expression does not cover all possible constructors of the ADT \
being matched. Every possible value must be handled.\n\
\n\
Common causes:\n\
• Forgetting a constructor variant\n\
• Missing a catch-all (`_`) arm\n\
• The ADT was extended with new constructors",
            example: "\
type Color = Red | Green | Blue\n\
\n\
fn name(c: Color) -> String {\n\
    match c {\n\
        Red => \"red\",\n\
        Green => \"green\",\n\
        // error: non-exhaustive — missing `Blue`\n\
    }\n\
}",
            see_also: &["UnreachableArm"],
        },

        "UnreachableArm" => ErrorExplanation {
            name: "UnreachableArm",
            code: "W0001",
            category: "Pattern Matching",
            summary: "pattern is unreachable",
            detail: "\
A match arm can never be reached because a previous arm already covers \
all values that would match it.\n\
\n\
Common causes:\n\
• A catch-all (`_`) pattern before specific patterns\n\
• Duplicate patterns\n\
• A more general pattern preceding a more specific one",
            example: "\
fn check(x: Bool) -> Nat {\n\
    match x {\n\
        _ => 0,\n\
        true => 1,    // error: unreachable pattern\n\
    }\n\
}",
            see_also: &["NonExhaustiveMatch", "DeadCodeAfterReturn"],
        },

        "PatternTooDeep" => ErrorExplanation {
            name: "PatternTooDeep",
            code: "E0103",
            category: "Pattern Matching",
            summary: "pattern nesting exceeds limit",
            detail: "\
The pattern is nested too deeply. This limit prevents stack overflow \
during pattern compilation and ensures reasonable compile times.\n\
\n\
Common causes:\n\
• Deeply nested constructor patterns\n\
• Patterns that could be simplified with intermediate `let` bindings",
            example: "\
// Deeply nested pattern:\n\
match x {\n\
    Cons(1, Cons(2, Cons(3, Cons(4, ...))))    // error: too deep\n\
}\n\
\n\
// Fix: use intermediate bindings\n\
let inner = get_tail(get_tail(x));\n\
match inner { ... }",
            see_also: &["UnsupportedPattern"],
        },

        "MatchScrutineeNotAdt" => ErrorExplanation {
            name: "MatchScrutineeNotAdt",
            code: "E0022",
            category: "Pattern Matching",
            summary: "match arms expect an ADT scrutinee",
            detail: "\
The arms of this match are constructor patterns of an ADT, but the value \
being matched does not have that ADT's type.\n\
\n\
Common causes:\n\
• Matching constructors of one type against a value of another\n\
• A scrutinee whose inferred type is a primitive (`Nat`, `String`)\n\
• Forgetting to unwrap a value before matching on its contents",
            example: "\
type Opt = None | Some(Nat)\n\
\n\
fn f(n: Nat) -> Nat {\n\
    match n {\n\
        Some(x) => x,    // error: arms are `Opt` constructors, `n` is `Nat`\n\
        _ => 0,\n\
    }\n\
}",
            see_also: &["NonExhaustiveMatch", "UnsupportedPattern"],
        },

        "UnsupportedPattern" => ErrorExplanation {
            name: "UnsupportedPattern",
            code: "E0021",
            category: "Pattern Matching",
            summary: "pattern form not supported",
            detail: "\
This pattern form is not supported by the compiler.\n\
\n\
Common causes:\n\
• Using a pattern syntax that doesn't exist in Tungsten\n\
• Trying to match on types that don't support pattern matching",
            example: "\
// Pattern forms may vary by compiler version",
            see_also: &["NonExhaustiveMatch", "PatternTooDeep"],
        },

        _ => return None,
    };
    Some(exp)
}

fn named_record(name: &str) -> Option<ErrorExplanation> {
    let exp = match name {
        "NotARecordType" => ErrorExplanation {
            name: "NotARecordType",
            code: "E0050",
            category: "Named Records",
            summary: "type is not a record",
            detail: "\
A named record constructor `TypeName { field: value, ... }` was used with \
a type that is not a record type. Only record types (types defined with \
`{ field: Type, ... }` syntax) support named construction.",
            example: "\
type Color = Red | Green | Blue\n\
\n\
fn bad() -> Color {\n\
    Color { x: 1 }    // error: Color is an ADT, not a record\n\
}\n\
\n\
// Fix: use a constructor\n\
fn good() -> Color { Red }",
            see_also: &["MissingRecordField", "ExtraRecordField"],
        },

        "MissingRecordField" => ErrorExplanation {
            name: "MissingRecordField",
            code: "E0051",
            category: "Named Records",
            summary: "missing field in record constructor",
            detail: "\
A named record constructor is missing a required field. All fields defined \
in the record type must be provided.",
            example: "\
type Point = { x: Nat, y: Nat }\n\
\n\
fn bad() -> Point {\n\
    Point { x: 1 }    // error: missing field `y`\n\
}\n\
\n\
// Fix: provide all fields\n\
fn good() -> Point { Point { x: 1, y: 2 } }",
            see_also: &["ExtraRecordField", "NotARecordType"],
        },

        "ExtraRecordField" => ErrorExplanation {
            name: "ExtraRecordField",
            code: "E0052",
            category: "Named Records",
            summary: "unknown field in record constructor",
            detail: "\
A named record constructor includes a field that does not exist in the \
record type definition.",
            example: "\
type Point = { x: Nat, y: Nat }\n\
\n\
fn bad() -> Point {\n\
    Point { x: 1, y: 2, z: 3 }    // error: unknown field `z`\n\
}\n\
\n\
// Fix: remove the extra field\n\
fn good() -> Point { Point { x: 1, y: 2 } }",
            see_also: &["MissingRecordField", "NotARecordType"],
        },

        "DuplicateRecordField" => ErrorExplanation {
            name: "DuplicateRecordField",
            code: "E0053",
            category: "Named Records",
            summary: "field specified twice",
            detail: "\
A named record constructor specifies the same field more than once. \
Each field must appear exactly once.",
            example: "\
type Point = { x: Nat, y: Nat }\n\
\n\
fn bad() -> Point {\n\
    Point { x: 1, x: 2, y: 3 }    // error: duplicate field `x`\n\
}\n\
\n\
// Fix: remove the duplicate\n\
fn good() -> Point { Point { x: 1, y: 3 } }",
            see_also: &["MissingRecordField", "ExtraRecordField"],
        },

        _ => return None,
    };
    Some(exp)
}

fn entry_point(name: &str) -> Option<ErrorExplanation> {
    let exp = match name {
        "NoMainFunction" => ErrorExplanation {
            name: "NoMainFunction",
            code: "E0030",
            category: "Entry Point",
            summary: "no main function found",
            detail: "\
The file does not define a `main` function. When running or compiling a file, \
Tungsten requires a `fn main()` as the entry point.\n\
\n\
Note: `tungsten check` does NOT require a main function — it only type-checks.",
            example: "\
// This file has no main function\n\
fn helper() -> Nat { 42 }\n\
\n\
// Fix: add a main function\n\
fn main() -> Nat { helper() }",
            see_also: &["ContainsSorry"],
        },

        "ContainsSorry" => ErrorExplanation {
            name: "ContainsSorry",
            code: "E0031",
            category: "Entry Point",
            summary: "file contains sorry (cannot compile)",
            detail: "\
The file contains `sorry`, which is a placeholder for unfinished proofs. \
Files with `sorry` can be type-checked but cannot be compiled or run, \
because `sorry` has no runtime semantics.\n\
\n\
`sorry` is useful during development to skip proofs temporarily, \
but must be replaced with actual proofs before compilation.",
            example: "\
theorem add_comm(a: Nat, b: Nat) : Eq Nat (a + b) (b + a) {\n\
    sorry    // placeholder — cannot compile\n\
}",
            see_also: &["NoMainFunction"],
        },

        _ => return None,
    };
    Some(exp)
}
