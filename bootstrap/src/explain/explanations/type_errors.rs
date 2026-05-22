//! Type error explanations.
//!
//! Handles: `TypeMismatch`, `CannotInferType`, `CannotInferTypeArg`,
//! `ArityMismatch`, `ExpectedFunction`, `ExpectedType`.

use crate::explain::error_catalogue::ErrorExplanation;

pub(super) fn type_errors(name: &str) -> Option<ErrorExplanation> {
    inference_errors(name).or_else(|| application_errors(name))
}

/// Type inference errors: mismatch, cannot infer.
fn inference_errors(name: &str) -> Option<ErrorExplanation> {
    let exp = match name {
        "TypeMismatch" => ErrorExplanation {
            name: "TypeMismatch",
            category: "Type Errors",
            summary: "expected one type, found another",
            detail: "\
The compiler expected one type but found a different type. This is the \
most common error in Tungsten programs.\n\
\n\
Common causes:\n\
• Returning the wrong type from a function body\n\
• Passing an argument of the wrong type to a function\n\
• Match arms returning different types\n\
• Using a constructor with wrong field types\n\
\n\
Structural types in this error:\n\
Tungsten's Core IR uses structural encodings for ADTs. If you see \
a μ-type in a TypeMismatch error, use `tungsten explain type` to decode it:\n\
  $ tungsten explain type \"μα_List. (Unit + (Nat × α_List))\"",
            example: "\
fn greet() -> String {\n\
    42              // error: expected `String`, found `Nat`\n\
}",
            see_also: &["CannotInferType", "ExpectedType", "ExpectedFunction"],
        },

        "CannotInferType" => ErrorExplanation {
            name: "CannotInferType",
            category: "Type Errors",
            summary: "type annotation needed",
            detail: "\
The compiler cannot determine the type of an expression from context alone. \
An explicit type annotation is needed.\n\
\n\
Common causes:\n\
• Variable declaration without type annotation in ambiguous context\n\
• Generic function call where type arguments can't be inferred",
            example: "\
fn id<T>(x: T) -> T { x }\n\
\n\
fn main() -> Nat {\n\
    let y = id(42);    // may need: let y: Nat = id(42)\n\
    y\n\
}",
            see_also: &["CannotInferTypeArg", "TypeMismatch"],
        },

        "CannotInferTypeArg" => ErrorExplanation {
            name: "CannotInferTypeArg",
            category: "Type Errors",
            summary: "cannot infer type argument",
            detail: "\
The compiler cannot infer a type argument for a polymorphic (generic) function. \
Provide explicit type arguments.\n\
\n\
Common causes:\n\
• Calling a generic function in a context without enough type information\n\
• The result type doesn't constrain the type parameter",
            example: "\
fn empty_list<T>() -> List<T> { Nil }\n\
\n\
fn main() -> List<Nat> {\n\
    empty_list()    // error: cannot infer type argument `T`\n\
    // Fix: empty_list::<Nat>()\n\
}",
            see_also: &["CannotInferType"],
        },

        _ => return None,
    };
    Some(exp)
}

/// Application and arity errors.
fn application_errors(name: &str) -> Option<ErrorExplanation> {
    let exp = match name {
        "ArityMismatch" => ErrorExplanation {
            name: "ArityMismatch",
            category: "Type Errors",
            summary: "wrong number of arguments",
            detail: "\
A function was called with the wrong number of arguments.\n\
\n\
Common causes:\n\
• Forgetting an argument\n\
• Passing too many arguments\n\
• Confusing two functions with similar names but different arities",
            example: "\
fn add(x: Nat, y: Nat) -> Nat { x + y }\n\
\n\
fn main() -> Nat {\n\
    add(1, 2, 3)    // error: expected 2 arguments, found 3\n\
}",
            see_also: &["TypeMismatch", "ExpectedFunction"],
        },

        "ExpectedFunction" => ErrorExplanation {
            name: "ExpectedFunction",
            category: "Type Errors",
            summary: "expected function, found other type",
            detail: "\
An expression was used as a function (called with arguments), but its type \
is not a function type.\n\
\n\
Common causes:\n\
• Calling a non-function value (e.g., a Nat or a constructor used wrong)\n\
• Typo causing a variable to shadow a function name\n\
• Missing parentheses in a chain of calls",
            example: "\
fn main() -> Nat {\n\
    let x: Nat = 42;\n\
    x(1)    // error: expected function, found `Nat`\n\
}",
            see_also: &["TypeMismatch", "ArityMismatch"],
        },

        "ExpectedType" => ErrorExplanation {
            name: "ExpectedType",
            category: "Type Errors",
            summary: "expected a specific type",
            detail: "\
The compiler expected a specific type (like Bool for an if-condition) \
but found a different type.\n\
\n\
Common causes:\n\
• Using a non-Bool expression as an if-condition\n\
• Type annotation doesn't match the expression",
            example: "\
fn main() -> Nat {\n\
    if 42 { 1 } else { 0 }    // error: expected `Bool`, found `Nat`\n\
}",
            see_also: &["TypeMismatch"],
        },

        _ => return None,
    };
    Some(exp)
}
