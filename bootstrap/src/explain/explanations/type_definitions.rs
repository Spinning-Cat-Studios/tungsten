//! `Type Definitions` explanations: the recursive-type-shape rejections.
//!
//! Split out of `mod.rs` by ADR 11.8.26c, which added E0064 and took the file
//! over its size threshold. This is the same per-category seam
//! `control_flow` / `equality` / `termination` already use — the three codes
//! here (E0060, E0061, E0064) are the ones a reader reaches for when a
//! recursive type is refused, so they belong together.

use crate::explain::error_catalogue::ErrorExplanation;

/// Type-definition errors (ADR 7.8.26e, extended by 11.8.26c).
pub(super) fn type_definitions(name: &str) -> Option<ErrorExplanation> {
    let exp = match name {
        "RecursiveAlias" => ErrorExplanation {
            name: "RecursiveAlias",
            code: "E0060",
            category: "Type Definitions",
            summary: "type alias references itself",
            detail: "\
A type alias is *substituted*, not defined — expanding one that mentions \
itself never terminates, so the cycle is rejected at definition time.\n\
\n\
This is not a restriction on recursive types; it is a restriction on aliases. \
An ADT may reference itself freely, because its constructors give the \
recursion a place to stop. If you wanted a recursive type, define one:\n\
\n\
Cycles through several aliases (`A = B`, `B = A`) are the same error and are \
reported on whichever the elaborator reaches first.",
            example: "\
type Foo = Foo                          // error: alias expands forever\n\
type List = Nil | Cons(Nat, List)       // fine: an ADT, not an alias",
            see_also: &["NonStrictlyPositive", "UndefinedType"],
        },

        "NonStrictlyPositive" => ErrorExplanation {
            name: "NonStrictlyPositive",
            code: "E0061",
            category: "Type Definitions",
            summary: "type is not strictly positive",
            detail: "\
A type definition is *strictly positive* when its own recursive occurrences \
never appear to the left of an arrow, at any depth. Tungsten requires this, \
because it is what makes \"structural subterm\" a well-founded relation — the \
relation termination checking rests on.\n\
\n\
Without the restriction a type can encode a fixpoint combinator, yielding a \
diverging term with no syntactic recursion anywhere, and at `Void` a closed \
inhabitant of the empty type.\n\
\n\
Note this is *strict* positivity, not positivity: two occurrences to the left \
of an arrow do not cancel out. `Mk((T -> Nat) -> Nat)` is rejected too.\n\
\n\
A violation can also be inherited. If `Fn1<T>` uses `T` to the left of an \
arrow, then `B(Fn1<Bad>)` places `Bad` in a forbidden position even though no \
arrow appears in `Bad`'s own source. The message names the intermediate type \
and parameter when that is what happened.\n\
\n\
There is no escape hatch: a rejected type is fixed, not annotated. The usual \
fix is to store the *result* of the function rather than the function itself, \
or to break the cycle with an index into a separately-owned collection.",
            example: "\
type Bad = Mk(Bad -> Bad)          // error: `Bad` occurs left of an arrow\n\
type Bad3 = Mk((Bad3 -> Nat) -> Nat)   // error: positive, but not strictly\n\
\n\
type Ok = Mk(Nat -> Ok)            // fine: only the codomain\n\
type Tree = Node(List<Tree>)       // fine: `List` uses its parameter strictly",
            see_also: &["NestedRecursiveFamily"],
        },

        "NestedRecursiveFamily" => ErrorExplanation {
            name: "NestedRecursiveFamily",
            code: "E0064",
            category: "Type Definitions",
            summary: "recursion nested under a generic parameter",
            detail: "\
A type is a *nested* inductive family when its recursive occurrence sits \
under a generic parameter — `Wrap<Rose>` rather than `Rose`. Phase 1 does \
not support matching on one.\n\
\n\
The reason is the encoding. A nested occurrence has no place to put the \
recursive reference, so the μ-encoding collapses to a vacuous binder \
(`Rose` encodes to `μα_Rose. α_Rose`) which never unfolds to a sum type — \
and constructor arms need a sum type to be elaborated against. The message \
names that binder, because the tool that would otherwise show it \
(`tungsten info type type-encoding <T>`) cannot run on a file this gate \
rejects — delete the `match` first if you want to see the whole chain.\n\
\n\
The type *definition* is accepted; only a `match` on it is rejected. The \
fix is to break the nesting with a non-generic intermediate type, which \
gives the recursion a concrete constructor to stop at.\n\
\n\
Ordinary and mutual recursion are unaffected, and so is a generic ADT whose \
parameter is instantiated at a non-recursive type (`Wrap<Nat>`).",
            example: "\
type Wrap<T> = W(T)\n\
type Rose = Node(Wrap<Rose>)       // definition is fine\n\
match r { Node(w) => ... }         // error[E0064]: nested under `Wrap`\n\
\n\
// Fix: a non-generic intermediate type\n\
type RoseKids = NoKids | Kid(Rose, RoseKids)\n\
type Rose = Node(RoseKids)         // an ordinary mutual pair",
            see_also: &["NonStrictlyPositive", "RecursiveAlias"],
        },
        _ => return None,
    };
    Some(exp)
}
