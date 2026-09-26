//! Equality-proof errors (ADRs 21.5.26d, 21.5.26g), catalogued by ADR 8.8.26a.
//!
//! These nine kinds had codes (`E0070`–`E0078`) and rendered messages but no
//! `explain` entry, so `tungsten explain error E0070` answered "Unknown" for a
//! code the compiler itself had just printed.

use super::super::error_catalogue::ErrorExplanation;

/// Dispatch across the two halves below.
pub(super) fn equality_proofs(name: &str) -> Option<ErrorExplanation> {
    eq_forms(name).or_else(|| motives(name))
}

/// `refl` / `subst` / `trans` / `cong` — the equality forms themselves.
fn eq_forms(name: &str) -> Option<ErrorExplanation> {
    let exp = match name {
        "ReflExpectedEquality" => ErrorExplanation {
            name: "ReflExpectedEquality",
            code: "E0070",
            category: "Equality Proofs",
            summary: "refl checked against a non-equality type",
            detail: "\
`refl` is the only introduction form for the equality type `Eq τ a b`, so it \
can only be checked against one. The expected type here was something else.\n\
\n\
Usually the annotation is missing or wrong: `refl` cannot be inferred, because \
nothing in `refl` says which type or which terms it is proving equal.",
            example: "\
let p: Eq Nat 2 2 = refl;      // fine\n\
let q: Nat = refl;             // error: `Nat` is not an equality type",
            see_also: &["InvalidRefl", "SubstExpectedEquality"],
        },

        "InvalidRefl" => ErrorExplanation {
            name: "InvalidRefl",
            code: "E0071",
            category: "Equality Proofs",
            summary: "refl's two sides are not definitionally equal",
            detail: "\
`refl : Eq τ a a` proves a term equal to *itself*. The expected type asked it \
to prove two terms equal that do not reduce to the same normal form.\n\
\n\
Note the check is definitional, not propositional: `2 + 2` and `4` are \
accepted because they evaluate alike, but `n + 0` and `n` are not — that needs \
induction (`natind`), because it holds for every `n` without either side \
reducing.",
            example: "\
let p: Eq Nat (2 + 2) 4 = refl;    // fine: both normalize to 4\n\
let q: Eq Nat 3 4 = refl;          // error: 3 and 4 differ",
            see_also: &["ReflExpectedEquality", "NatIndMotiveNotNat"],
        },

        "SubstExpectedEquality" => ErrorExplanation {
            name: "SubstExpectedEquality",
            code: "E0072",
            category: "Equality Proofs",
            summary: "subst's proof argument is not an equality",
            detail: "\
`subst` rewrites one side of an equality into a goal, so its proof argument \
must have type `Eq τ a b`. It was given something else.\n\
\n\
A common cause is passing the *value* where the proof was expected, or passing \
a proof that has already been consumed by an earlier rewrite.",
            example: "\
subst [Nat] [motive] proof value;   // proof : Eq Nat a b\n\
subst [Nat] [motive] 42 value;      // error: `42` is not a proof",
            see_also: &["ReflExpectedEquality", "MotiveNotPredicate"],
        },

        "TransEndpointMismatch" => ErrorExplanation {
            name: "TransEndpointMismatch",
            code: "E0073",
            category: "Equality Proofs",
            summary: "trans endpoints do not meet",
            detail: "\
`trans` chains `Eq τ a b` and `Eq τ b c` into `Eq τ a c`. The two proofs must \
meet at the same middle term `b`; here the first ends somewhere the second \
does not start.\n\
\n\
The message prints both endpoints. When they look identical, they differ \
structurally rather than in rendering — the same trap E0010 documents.",
            example: "\
trans ab bc;    // ab : Eq Nat a b, bc : Eq Nat b c  -> Eq Nat a c\n\
trans ab cd;    // error: `b` (end of ab) is not `c` (start of cd)",
            see_also: &["InvalidRefl", "TypeMismatch"],
        },

        "CongExpectedFunction" => ErrorExplanation {
            name: "CongExpectedFunction",
            code: "E0074",
            category: "Equality Proofs",
            summary: "cong's first argument is not a function",
            detail: "\
`cong f p` lifts a proof `p : Eq τ a b` through a function, yielding \
`Eq σ (f a) (f b)`. Its first argument must therefore *be* a function; here it \
was a value.\n\
\n\
Congruence is what lets you rewrite under a constructor or an arithmetic \
operator, so `f` is usually a lambda naming the position to rewrite.",
            example: "\
cong (|x: Nat| x + 1) p;    // p : Eq Nat a b  ->  Eq Nat (a+1) (b+1)\n\
cong 5 p;                   // error: `5` is not a function",
            see_also: &["ExpectedFunction", "SubstExpectedEquality"],
        },

        _ => return None,
    };
    Some(exp)
}

/// The motive arguments to `subst` and `natind` — the one part of an equality
/// proof the elaborator cannot infer, and therefore the one users get wrong.
fn motives(name: &str) -> Option<ErrorExplanation> {
    let exp = match name {
        "MotiveNotPredicate" => ErrorExplanation {
            name: "MotiveNotPredicate",
            code: "E0075",
            category: "Equality Proofs",
            summary: "subst motive is not a predicate lambda",
            detail: "\
A motive tells `subst` *where* in the goal to rewrite, so it must be a lambda \
from the equality's base type to a type: `|x: τ| <type mentioning x>`. \
Something else was supplied.\n\
\n\
The motive is the one argument that cannot be inferred — the elaborator cannot \
tell which occurrences of `a` you meant to rewrite and which to leave.",
            example: "\
subst [Nat] [|x: Nat| Eq Nat x 5] p q;    // fine\n\
subst [Nat] [Nat] p q;                    // error: `Nat` is not a lambda",
            see_also: &["MotiveDomainMismatch", "MotiveBodyNotType"],
        },

        "MotiveDomainMismatch" => ErrorExplanation {
            name: "MotiveDomainMismatch",
            code: "E0076",
            category: "Equality Proofs",
            summary: "motive parameter type differs from the equality's base type",
            detail: "\
The motive's parameter is what the equality's two sides get substituted for, \
so its type must be the equality's base type `τ`. Here the motive binds a \
different type.\n\
\n\
Reading the message: `expected` is the equality's base type, `found` is what \
the motive's binder declared.",
            example: "\
// p : Eq Nat a b\n\
subst [Nat] [|x: Nat| ...] p q;      // fine\n\
subst [Nat] [|x: Bool| ...] p q;     // error: motive binds Bool, equality is over Nat",
            see_also: &["MotiveNotPredicate", "TypeMismatch"],
        },

        "MotiveBodyNotType" => ErrorExplanation {
            name: "MotiveBodyNotType",
            code: "E0077",
            category: "Equality Proofs",
            summary: "motive body is a term, not a type",
            detail: "\
A motive's body is the *goal shape* to rewrite in, so it must be a type \
expression. Here it is a term.\n\
\n\
The usual slip is writing the value you want rather than the proposition about \
it — `|x: Nat| x` instead of `|x: Nat| Eq Nat x 5`.",
            example: "\
subst [Nat] [|x: Nat| Eq Nat x 5] p q;    // fine: body is a type\n\
subst [Nat] [|x: Nat| x] p q;             // error: body is a term",
            see_also: &["MotiveNotPredicate", "MotiveDomainMismatch"],
        },

        "NatIndMotiveNotNat" => ErrorExplanation {
            name: "NatIndMotiveNotNat",
            code: "E0078",
            category: "Equality Proofs",
            summary: "natind motive does not range over Nat",
            detail: "\
`natind` is induction on the natural numbers, so its motive must be a \
predicate on `Nat` — `P : Nat -> Prop`. The motive supplied ranges over \
something else.\n\
\n\
Induction is what proves the facts `refl` cannot: a statement true for every \
`n` where neither side reduces, such as `n + 0 = n`.",
            example: "\
natind [|n: Nat| Eq Nat (n + 0) n] base step k;    // fine\n\
natind [|b: Bool| ...] base step k;                // error: motive is over Bool",
            see_also: &["MotiveNotPredicate", "InvalidRefl"],
        },

        _ => return None,
    };
    Some(exp)
}
