//! Explanations for the termination gate's error codes (ADR 29.6.26e).

use crate::explain::error_catalogue::ErrorExplanation;

/// `tungsten explain error E0062` / `E0063`.
pub(super) fn termination(name: &str) -> Option<ErrorExplanation> {
    let exp = match name {
        "CannotProveTermination" => ErrorExplanation {
            name: "CannotProveTermination",
            code: "E0062",
            category: "Termination",
            summary: "recursion is not structurally decreasing",
            detail: "\
A non-terminating function inhabits every type, including the empty one, so a \
recursive definition is admitted only once the compiler can see that it stops. \
Phase 1 checks the structural rule: some parameter must get **strictly \
smaller** at every recursive call.\n\
\n\
\"Strictly smaller\" means bound by taking a value apart, not built back up. \
Matching `l` against `Cons(h, t)` proves `t` smaller than `l`; `Cons(0, t)` is \
not smaller than `l`, even though `t` is. Aliases and projections of a smaller \
value stay smaller; rebinding the name loses the fact.\n\
\n\
For mutual recursion the decrease is measured against the **caller's** \
parameter. It is not enough for each call to fill the callee's decreasing \
position — a group that passes its argument round unchanged occupies every \
position correctly and never makes progress.\n\
\n\
The recursion also has to be visible as a call. `let g = f; g(x)` recurses \
through a value, and there is no call site to inspect, so it is rejected \
rather than assumed fine.\n\
\n\
When more than one parameter decreases at every call, the compiler will not \
guess: write `#[decreasing(arg)]`. When the recursion genuinely is not \
structural — an accumulator, a measure, a division — Phase 1 cannot certify \
it. Mark it `#[partial]` and it is admitted as an opaque constant that proofs \
may not use (see PartialInProof).",
            example: "\
fn len(l: List) -> Nat {                     // fine: `t` is smaller than `l`\n\
    match l { Nil => 0, Cons(h, t) => 1 + len(t) }\n\
}\n\
\n\
fn spin(l: List) -> Nat { spin(l) }          // error: `l` never shrinks\n\
\n\
#[partial]\n\
fn collatz(n: Nat) -> Nat { … }              // opt out; unusable in proofs",
            see_also: &["PartialInProof", "NonStrictlyPositive"],
        },

        "PartialInProof" => ErrorExplanation {
            name: "PartialInProof",
            code: "E0063",
            category: "Termination",
            summary: "a proof depends on a partial definition",
            detail: "\
`#[partial]` buys executable code an exemption from termination checking. It \
cannot buy a proof one: a proof that may reference a possibly-diverging \
constant proves nothing.\n\
\n\
The restriction is transitive, which is the point. A wrapper that merely calls \
a partial function is itself partial, so routing a theorem through one more \
layer does not launder it. The message names the chain it travelled.\n\
\n\
A definition's **type** counts as well as its body — an equality statement \
embeds terms, so a theorem can reach a partial constant without its proof term \
mentioning one.\n\
\n\
The fix is to prove the recursion terminates (rewrite it structurally, or name \
the decreasing parameter), or to state the theorem about a definition that is \
already total. Executable code is unaffected: a partial constant is welcome \
anywhere outside a proof.",
            example: "\
#[partial]\n\
fn search(k: Nat) -> Nat { … }\n\
\n\
fn wrapper(k: Nat) -> Nat { search(k) }   // fine — and now partial too\n\
\n\
theorem found(k: Nat) : Eq Nat (wrapper(k)) k = refl\n\
//      error: reaches `search` (marked #[partial]) through wrapper",
            see_also: &["CannotProveTermination", "ContainsSorry"],
        },

        _ => return None,
    };
    Some(exp)
}
