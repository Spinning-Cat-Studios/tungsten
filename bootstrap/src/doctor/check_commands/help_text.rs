//! Long `--help` prose for `doctor check` subcommands whose explanations are
//! longer than their declarations.
//!
//! **Why prose lives here.** `check_commands/mod.rs` holds one clap enum, and
//! an enum cannot be split across files — so when ADR 13.8.26c's
//! `name-collisions` variant pushed it past the 400-line cap, the only seam
//! that means anything is *shape versus prose*. `mod.rs` now carries the
//! variants, their flags and their `after_help` cross-references; the long
//! `long_about` bodies live here, byte for byte.
//!
//! Longest-first, and only as far as the cap required: the remaining variants
//! keep their prose inline until the same pressure recurs. A doc comment still
//! supplies each variant's one-line `about`, so `-h` is unchanged and only
//! `--help` reads from this file.

/// `doctor check module name-collisions` (ADR 13.8.26c).
pub const NAME_COLLISIONS: &str = "Report value names defined in more than one reachable module\n\
(ADR 13.8.26c)\n\
\n\
Both compilers key their value environments on the **bare name**, so the\n\
module-tree walk's last registration wins and, when the winner is\n\
private, every call site of the loser reports E0016 *in the loser's\n\
file, naming the winner's module* — nowhere near the edit that caused\n\
it, once per call site (77 for one collision in ADR 7.8.26b).\n\
\n\
Every finding names both modules AND marks which one currently wins, the\n\
two facts E0016 withholds. Three classes, because their fixes differ:\n\
`extern-symbol` (a duplicate C symbol — no visibility change helps),\n\
`private-shadowed` (the live E0016 class), `latent-public` (resolves\n\
today, breaks the day the loser is called). `--severity live` selects\n\
the first two.\n\
\n\
**Parse-only (cost 2), so it runs on a file the elaborator REJECTS** —\n\
the only state in which anyone needs it. **Advisory: exit is 0 even with\n\
findings**, under `--severity live` as much as under `all`; read\n\
`--json` to gate on them.\n\
\n\
Examples:\n\
  tungsten doctor check module name-collisions src/compiler/main.tg\n\
  tungsten doctor check module name-collisions main.tg --severity live\n\
  tungsten doctor check module name-collisions main.tg --json";

/// `doctor check comparable` (ADRs 1.8.26b, 1.8.26c).
pub const COMPARABLE: &str =
    "Report whether the structural comparator can handle a type, and where\n\
it breaks (ADRs 1.8.26b, 1.8.26c)\n\
\n\
Surfaces the gate's own verdict — the same decision the `__cmp<T>`\n\
callback makes — so the diagnostic cannot disagree with what a run does.\n\
Three classes, because their fixes differ: an opaque leaf (noncomparable\n\
by policy), an incomplete closure (a `compare_*` symbol the closure\n\
never defines, which since 1.8.26b D2 is what an unresolvable μ-cluster\n\
member looks like), and unsettled synthesis (the closure walk hit its\n\
bound, or a chain of nested generic instantiations exceeded its\n\
expansion depth).\n\
\n\
Since 1.8.26c a generic ADT instantiation such as `List<TypeParam>`\n\
RESOLVES rather than being an opaque leaf, so an opaque leaf naming an\n\
`App(...)` now means a generic alias or record — both still out of\n\
scope. Measured on the compiler's own AST graph, that arm took `--all`\n\
from 109 comparable / 178 opaque / 47 incomplete to 245 / 0 / 0.\n\
\n\
Run it before writing assertions at a new type: it is the cheaper order,\n\
though no longer the only safeguard — since D3, a comparison that never\n\
ran is reported NEVER COMPARED and fails the run instead of passing\n\
silently. Cost 3 (elaboration only). Exit 1 on findings, 2 on bad input.\n\
\n\
`--all` checks every type the file declares in ONE elaboration. On the\n\
compiler's own module graph elaboration is ~88 s of a ~147 s run, so\n\
asking about four types one at a time costs far more than asking about\n\
all of them.\n\
\n\
CLEAN THE CACHE FIRST. `record_types` and `adt_types` are not cached, so\n\
on an elaboration-cache hit every input map is empty and the whole\n\
corpus reports noncomparable — a silent, total false positive.\n\
\n\
Examples:\n\
  tungsten cache clean\n\
  tungsten doctor check comparable Pattern src/compiler/test_ast_compare.tg\n\
  tungsten doctor check comparable TypeExpr src/compiler/main.tg\n\
  tungsten doctor check comparable --all src/compiler/test_ast_compare.tg";

/// `doctor check tco-coverage` (ADRs 1.7.26b, 1.7.26e).
pub const TCO_COVERAGE: &str =
    "Rank self-recursive functions by O(N)-stack risk from the *actual*\n\
codegen musttail decision (ADR 1.7.26b).\n\
\n\
Compiles with codegen, collects every musttail EMIT/SKIP/DECOMPOSE\n\
decision at the `check_musttail_abi_safety` gate, and prints a ranked\n\
table (or JSON) of self-recursive functions that skip musttail.\n\
\n\
See also: `tungsten info codegen musttail-eligibility` (per-function\n\
drill-down), `tungsten info codegen indirect-abi` (Class-P lowering),\n\
`tungsten doctor audit-recursion` (source-level view).\n\
\n\
Examples:\n\
  tungsten doctor check tco-coverage src/compiler/main.tg\n\
  tungsten doctor check tco-coverage main.tg --json\n\
  tungsten doctor check tco-coverage main.tg --risk high\n\
  tungsten doctor check tco-coverage main.tg --gate   # CI gate (exit≠0 on SKIP)";

/// `doctor check sorry-sites` (ADR 18.9.26g).
pub const SORRY_SITES: &str = "List every definition carrying a proof hole, and who put it there\n\
(ADR 18.9.26g)\n\
\n\
`tungsten check` prints `contains sorry (A authored, S synthesised)` and\n\
names no definition. This is the census behind that line: one row per\n\
definition whose Core term carries a `Sorry`, each hole classified:\n\
\n\
  authored      the author wrote `sorry` (or an `axiom`), at file:line:col\n\
  synthesised   the pattern lowering planted it, named by construct:\n\
                `absurd branch` (a nested pattern on a two-constructor\n\
                type such as `Cons(x, Nil())`) or `unreachable pattern arm`\n\
                (three or more constructors). Sound: the arm is unreachable\n\
  unclassified  any other hole the author did not write. One known\n\
                producer: `==`/`!=` on a type with no equality primitive,\n\
                which lowers to a hole with NO diagnostic\n\
\n\
Cost 3 (elaborate), no codegen feature: the route that lists names on\n\
`compile` needs LLVM. Findings are not failures: exit 0, as `check` passes\n\
a program with a hole. `--json` for tooling. A cache written before ADR\n\
18.9.26g holds unmarked authored holes, which read `unclassified` until\n\
`tungsten cache clean`.\n\
\n\
Examples:\n\
  tungsten doctor check sorry-sites src/compiler/main.tg\n\
  tungsten doctor check sorry-sites main.tg --json\n\
";
