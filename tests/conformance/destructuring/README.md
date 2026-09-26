# The destructuring conformance corpus (ADR 4.9.26c)

Small `.tg` programs run through **both** compilers by
`selfhost-conformance --exec`, which compares the **answers** rather than the
verdicts. The soundness corpus next door asks "did they both accept?"; this one
asks "did they both mean the same thing?" — the cell the last two destructuring
defects lived in, where both compilers accept and one of them is wrong.

Every entry is a **pair** (ADR 4.9.26c D3): a *subject* that exercises the
construct where two candidate conventions stop coinciding, and a *control* that
takes the other path. A lone failing subject says something is broken; the pair
says where. The manifest that binds each pair together, and records which
convention a divergence would refute, is
`tools/selfhost-conformance/src/exec/corpus.rs` — not this directory, so a
fixture that is never indexed fails the build rather than sitting here unrun.

The corpus is indexed by what **distinguishes conventions**, not by feature
(D4): arity 1, 2 and 3 × a scalar tail and a product tail × pattern position
and field position.

| Directory | Construct | The two paths a pair separates |
|---|---|---|
| `field/` | `r.f` on a record | stop at the field COUNT vs stop when the residual type is no longer a product (ADR 4.9.26b) |
| `pattern/` | `let (a, b, …) = e` | expression position (`ExprLet`) vs block-statement position (`StmtLet`) — different elaborator paths for the same destructuring (ADR 3.9.26g §1.4) |
| `constructor/` | `match w { Mk(x) => … }` on a single-constructor type | one field, which unwraps to its payload (a bare scalar scrutinee), vs two fields, which encode to a product — the match dispatcher's arms for the two (ADR 18.9.26d) |
| `int_match/` | `match n { -1 => …, 0 \| 7 => …, x if … => …, _ => … }` on an `Int` or `Nat` | the integer match's conditional chain vs the same selections through `if` chains — nothing is taken apart, so the axis is arm selection: first-match-wins, guard scoping, negative-literal equality (ADR 18.9.26e) |

Each file is `fn main() -> Nat` returning a digit-packed encoding of the pieces
it destructured, so a mis-projection changes the printed number rather than
merely the shape of a term. A wrong projection that happens to stay well-typed
— `fst` of a genuine product — is exactly the defect
`doctor check selfhost well-typed-terms` cannot see, and it is visible here.
