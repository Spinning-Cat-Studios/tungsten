# Tungsten Benchmark Suite

Measures runtime performance of compiled Tungsten programs against structurally
equivalent Rust implementations.

## Quick Start

```bash
# Requires devcontainer with LLVM + hyperfine
make devcontainer-up
make devcontainer-benchmark
```

## What This Measures

Each benchmark pair (`.tg` + `.rs`) computes the same result using the same
algorithm and data representation. The driver verifies outputs match before
reporting timing ratios.

**Key principle:** Rust baselines are *structurally equivalent* (same allocation
pattern, same immutability constraints), not *idiomatically optimized*. The
question is "how much does Tungsten's codegen cost?" not "how much faster is
idiomatic Rust?"

## Benchmark Categories

| Category | What It Tests | Examples |
|----------|---------------|----------|
| Arithmetic | Peano ADT recursion, allocation | Fibonacci, Collatz |
| Data Structures | ADT allocation, pattern matching | List sum, map, reverse |
| Polymorphism | Monomorphization overhead | Generic map over different types |
| Closures | Closure allocation, environment capture | Higher-order function chains |
| Strings | FFI boundary cost | String concatenation, equality |

## Representation Rules

| Category | Tungsten | Rust Baseline |
|----------|----------|---------------|
| Peano Nat | `Nat` (Peano ADT) | `enum Nat { Zero, Succ(Box<Nat>) }` |
| List/Tree | `List<T>`, `Tree<T>` | `enum List<T> { Nil, Cons(T, Box<List<T>>`) }` |
| Strings | FFI through `tungsten_core` | Same FFI or `String` |

## Anti-Optimization Rules

1. **Consume results** — every benchmark prints its result to prevent DCE.
2. **Observable outputs** — results are printed (not discarded), so the
   compiler cannot eliminate the computation.
3. **Correctness check** — the driver verifies Tungsten and Rust produce
   identical output before accepting timing results.

## Compiler Self-Performance

Reported separately from runtime benchmarks. Measures how fast the compiler
itself runs (elaboration + codegen), not how fast compiled programs run.

## Interpreting Results

- **Ratio < 2x**: Near-parity — codegen is competitive.
- **Ratio 2–5x**: Expected for closure-heavy or ADT-heavy code.
- **Ratio 5–20x**: Worth investigating — may indicate optimization opportunity.
- **Ratio > 20x**: Likely a codegen deficiency worth prioritizing.

## Peano Nat Limits

Tungsten's `Nat` is a Peano ADT (`Zero | Succ(Nat)`). Peano arithmetic
operations (`add`, `mul`) recurse proportionally to the magnitude of
their operands — `mul(a, b)` recurses `O(a * b)` deep. This means:

- **Safe**: Nat values up to ~50,000 (stays within 8MB default stack)
- **Dangerous**: `factorial(9)` = 362,880 Succ nodes — borderline
- **Stack overflow**: `factorial(12)` = 479M Succ nodes — impossible

When adding arithmetic benchmarks, keep intermediate Peano values below
~50K. Collatz is ideal because peak values stay moderate (~9,232 for
input 27) across many steps.

## Environment Requirements

- devcontainer with LLVM 18
- `hyperfine` (installed in devcontainer or host)
- `rustc` (for Rust baselines)
- Consistent thermal/power state for reliable measurements
