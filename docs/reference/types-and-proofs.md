# Types and Proofs

**This document has moved to
<https://spinningcatstudios.com/tungsten/proofs/curry-howard>.**

*The linked pages were reconciled against the 2.0-alpha compiler on 2026-09-25.*

| You want | Go to |
|---|---|
| Types as propositions (Curry-Howard) | <https://spinningcatstudios.com/tungsten/proofs/curry-howard> |
| Writing theorems | <https://spinningcatstudios.com/tungsten/proofs/theorems> |
| Dependent types | <https://spinningcatstudios.com/tungsten/proofs/dependent-types> |
| Induction over naturals (`natind`) | <https://spinningcatstudios.com/tungsten/proofs/induction> |
| Data types and generics | <https://spinningcatstudios.com/tungsten/language/data-types> |

## A note on the version that used to be here

The previous contents were written against v1.0 and predate the v1.5 proof
surface — propositional equality (`Eq<T, a, b>` with `refl`), the `sym` / `trans`
/ `cong` / `subst` combinators, and the `natind` and `natrec` eliminators, none
of which it described.

It also documented turbofish (`id::<Nat>(42)`) as a working way to supply
explicit type arguments. It does not parse; annotate the binding or return type
instead.

One correction in the other direction: the old document was right that `Nat`
supports `-`, `*`, `/` and `%`, where `docs/reference/syntax.md` claimed it did
not. The replacement pages resolve the contradiction in favour of what the
compiler does.
