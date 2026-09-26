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
            code: "E0010",
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
            code: "E0011",
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
            code: "E0015",
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
            code: "E0012",
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
            code: "E0013",
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
            code: "E0014",
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

        "ComparatorUnavailable" => ErrorExplanation {
            name: "ComparatorUnavailable",
            code: "E0080",
            category: "Type Errors",
            summary: "no comparator for this type",
            detail: "\
`__compare` — which the `assert_eq_*` test assertions desugar to — was \
applied at a type the compiler cannot synthesize a structural comparator \
for.\n\
\n\
Supported: primitives, tuples, sums/ADTs, records, and lists. Function \
types are the usual culprit: two functions have no decidable equality.\n\
\n\
`tungsten doctor check comparable <type> <file>` reports whether a type \
is comparable before you write assertions at it.",
            example: "\
fn f(x: Nat) -> Nat { x }\n\
fn g(x: Nat) -> Nat { x }\n\
\n\
fn test_fns() -> Bool {\n\
    __compare(f, g)    // error: no comparator for `Nat -> Nat`\n\
}",
            see_also: &["TypeMismatch"],
        },

        "IntLiteralOutOfRange" => ErrorExplanation {
            name: "IntLiteralOutOfRange",
            code: "E0090",
            category: "Type Errors",
            summary: "integer literal does not fit `Int`",
            detail: "\
A literal checked against `Int` is outside the signed 64-bit range \
(-9223372036854775808 to 9223372036854775807).\n\
\n\
Literals default to `Nat`; a literal becomes `Int` only where the \
expected type says so, or under a unary minus. `Nat` reaches \
18446744073709551615, so a value that fits `Nat` can still be too large \
for `Int` — `to_int(n)` traps at runtime for the same values.",
            example: "\
fn big() -> Int {\n\
    9223372036854775808    // error: out of range for `Int`\n\
}",
            see_also: &["TypeMismatch"],
        },

        "BuiltinTypeRedefined" => ErrorExplanation {
            name: "BuiltinTypeRedefined",
            code: "E0091",
            category: "Type Errors",
            summary: "a `type` under a builtin type's name",
            detail: "\
A `type` definition uses the name of a builtin type (`Int`, `Nat`, `Bool`, \
`Unit`, `Void`, `Prop`, `String`). Builtin lookup precedes user types, so \
the definition would never be reached — every use of the name would still \
resolve to the builtin, silently.\n\
\n\
Pick another name. For a C `int` return type the compiler's own FFI module \
uses `CInt`.",
            example: "\
type Int = Nat    // error: cannot redefine the builtin type `Int`",
            see_also: &["DuplicateDefinition"],
        },

        _ => return None,
    };
    Some(exp)
}
