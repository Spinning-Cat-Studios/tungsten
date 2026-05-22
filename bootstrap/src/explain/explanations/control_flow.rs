//! Error explanations for control flow errors (return, ?, let-else).

use crate::explain::error_catalogue::ErrorExplanation;
pub(super) fn control_flow(name: &str) -> Option<ErrorExplanation> {
    let exp = match name {
        "DeadCodeAfterReturn" => ErrorExplanation {
            name: "DeadCodeAfterReturn",
            category: "Control Flow",
            summary: "unreachable code after return",
            detail: "\
Code after a `return` expression is unreachable and will never execute.\n\
\n\
The `return` expression exits the function immediately, so any \
statements or expressions after it in the same block are dead code.",
            example: "\
fn foo() -> Nat {\n\
    return 42;\n\
    100    // warning: unreachable code after `return`\n\
}",
            see_also: &["UnreachableArm"],
        },

        "TryOnNonTryType" => ErrorExplanation {
            name: "TryOnNonTryType",
            category: "Control Flow",
            summary: "? on non-Result/Option type",
            detail: "\
The `?` operator can only be used on `Result<T, E>` or `Option<T>` types. \
It was applied to a type that is neither.\n\
\n\
`?` desugars to a match that unwraps the success case (`Ok` or `Some`) \
and early-returns the error case (`Err` or `None`).",
            example: "\
fn foo() -> Result<Nat, String> {\n\
    let x: Nat = 42;\n\
    x?    // error: ? requires Result or Option, found Nat\n\
}",
            see_also: &["TryReturnMismatch", "TryOutsideReturnContext"],
        },

        "TryReturnMismatch" => ErrorExplanation {
            name: "TryReturnMismatch",
            category: "Control Flow",
            summary: "? return type mismatch",
            detail: "\
The `?` operator requires the enclosing function's return type to be \
compatible with the operand type:\n\
\n\
• `Result<T, E>?` requires the function to return `Result<_, E>` \
(error types must match).\n\
• `Option<T>?` requires the function to return `Option<_>`.\n\
\n\
This ensures the early-return value is type-safe.",
            example: "\
fn foo() -> Nat {\n\
    let x: Result<Nat, String> = Ok(42);\n\
    x?    // error: cannot use ? in function returning Nat\n\
}\n\
\n\
// Fix: return Result\n\
fn foo() -> Result<Nat, String> {\n\
    let x: Result<Nat, String> = Ok(42);\n\
    x?    // ok: function returns Result<_, String>\n\
}",
            see_also: &["TryOnNonTryType", "TryOutsideReturnContext"],
        },

        "TryOutsideReturnContext" => ErrorExplanation {
            name: "TryOutsideReturnContext",
            category: "Control Flow",
            summary: "? outside function body",
            detail: "\
The `?` operator can only be used inside a function or closure body \
where the return type is known. It was used at module scope or in \
a context without a return type.\n\
\n\
`?` desugars to an early return, which requires an enclosing function.",
            example: "\
// At module scope:\n\
let x = some_result()?;    // error: ? outside function body\n\
\n\
// Fix: use inside a function\n\
fn process() -> Result<Nat, String> {\n\
    let x = some_result()?;    // ok\n\
    Ok(x)\n\
}",
            see_also: &["TryOnNonTryType", "TryReturnMismatch"],
        },

        "LetElseNonDiverging" => ErrorExplanation {
            name: "LetElseNonDiverging",
            category: "Control Flow",
            summary: "let-else branch does not diverge",
            detail: "\
The `else` branch of a `let`-`else` statement must diverge — it must \
not return a value to the enclosing scope. Typically this means using \
`return` to exit the function early.\n\
\n\
The `else` branch runs when the pattern does not match, so it must \
exit the scope (e.g., via `return` or a diverging expression).",
            example: "\
fn foo(x: Option<Nat>) -> Nat {\n\
    let Some(v) = x else { 0 };    // error: else branch does not diverge\n\
\n\
    // Fix: use return\n\
    let Some(v) = x else { return 0 };\n\
    v\n\
}",
            see_also: &["LetElseIrrefutable"],
        },

        "LetElseIrrefutable" => ErrorExplanation {
            name: "LetElseIrrefutable",
            category: "Control Flow",
            summary: "irrefutable pattern in let-else",
            detail: "\
The pattern in a `let`-`else` statement is irrefutable — it always \
matches, making the `else` branch unreachable.\n\
\n\
Use a plain `let` binding instead, since the pattern can never fail.",
            example: "\
fn foo(x: Nat) -> Nat {\n\
    let y = x else { return 0 };    // warning: irrefutable pattern\n\
\n\
    // Fix: use a plain let\n\
    let y = x;\n\
    y\n\
}",
            see_also: &["LetElseNonDiverging"],
        },

        "IfLetIrrefutable" => ErrorExplanation {
            name: "IfLetIrrefutable",
            category: "Control Flow",
            summary: "irrefutable pattern in if let",
            detail: "\
The pattern in an `if let` expression is irrefutable — it always \
matches, so the condition is always true.\n\
\n\
Use a plain `let` binding and unconditional block instead.",
            example: "\
fn foo(x: Nat) -> Nat {\n\
    if let y = x { y }    // warning: irrefutable pattern\n\
    else { 0 }\n\
\n\
    // Fix: use a plain let\n\
    let y = x;\n\
    y\n\
}",
            see_also: &["LetElseIrrefutable"],
        },

        _ => return None,
    };
    Some(exp)
}
