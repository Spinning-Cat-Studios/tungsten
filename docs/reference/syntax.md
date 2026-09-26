# Tungsten Syntax Reference

**This document has moved to
<https://spinningcatstudios.com/tungsten/reference/syntax>.**

*The linked pages were reconciled against the 2.0-alpha compiler on 2026-09-25.*

| You want | Go to |
|---|---|
| Syntax cheat-sheet | <https://spinningcatstudios.com/tungsten/reference/syntax> |
| Built-in types and operators | <https://spinningcatstudios.com/tungsten/reference/builtins> |
| Command-line reference | <https://spinningcatstudios.com/tungsten/reference/cli> |
| Current limitations | <https://spinningcatstudios.com/tungsten/reference/limitations> |
| The language manual, with explanation | <https://spinningcatstudios.com/tungsten/language/values-and-types> |

## A note on the version that used to be here

The previous contents were written against v1.0 and were wrong in ways worth
recording, since old copies may still be in circulation. It listed as
limitations several things that were either fixed in v1.5 or never limitations
at all:

| Claim in the old document | Reality |
|---|---|
| No nested patterns in `match` | Works since v1.5 |
| No early `return` | Works since v1.5 |
| No spread/rest syntax for records | Works since v1.5 |
| Modules flattened to a single namespace | Fixed in v1.5 |
| No `else if` | **Never true** — `else if` has always worked |
| No `-`, `*`, `/`, `%` infix operators | **Never true** — all four work |
| Turbofish `id::<Nat>(42)` supported | **Never true** — it does not parse |

Each of these was checked against the compiler when the replacement pages were
written. See
<https://spinningcatstudios.com/tungsten/reference/limitations> for the
current, verified inventory.
