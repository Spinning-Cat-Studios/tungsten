# Arithmetic Benchmarks

## fibonacci

**Algorithm:** Naive recursive Fibonacci on Peano-encoded natural numbers.
**Recursion:** O(2^n) calls for fib(n). Both use non-tail-recursive double recursion.
**Allocation:** Each `Succ(n)` allocates one heap cell. Rust uses `Box<Nat>`, Tungsten uses implicit heap allocation.
**Branch structure:** Two-level match (Zero/Succ, then Zero/Succ on predecessor).
**Input size:** fib(25) = 75025 Peano cells constructed per call.
**Iteration:** 8 iterations (default), read from argv.
**Observable output:** Sum of `to_nat(fib(mk_25()))` across all iterations. Default: 600200.
**Known differences:** Rust uses `&Nat` (shared reference) for recursive calls; Tungsten passes by value with implicit sharing.

## collatz

**Algorithm:** Collatz sequence length on Peano-encoded natural numbers.
**Recursion:** Depth depends on starting value. Both use tail-style recursion with an accumulator.
**Allocation:** Peano arithmetic (`div2`, `mul3`, `add`) allocates heap cells per `Succ`.
**Branch structure:** Match on even/odd (via `is_even`), recursive call in both arms.
**Input size:** collatz(27) = 111 steps.
**Iteration:** 100 iterations (default), read from argv.
**Observable output:** Sum of `to_nat(collatz_length(mk_27()))` across all iterations. Default: 11100.
**Known differences:** Rust uses `&Nat` references; Tungsten passes by value.

## factorial

**Algorithm:** Naive recursive factorial on Peano-encoded natural numbers.
**Recursion:** O(n) recursive calls. Both use non-tail-recursive multiplication.
**Allocation:** Peano `mul` and `add` allocate O(result) heap cells per operation.
**Branch structure:** Match on Zero/Succ, recursive call on predecessor.
**Input size:** factorial(7) = 5040 Peano cells. Capped at 7 to avoid stack overflow from deep Peano arithmetic.
**Iteration:** 500 iterations (default), read from argv.
**Observable output:** Sum of `to_nat(factorial(mk_7()))` across all iterations. Default: 2520000.
**Known differences:** Rust uses `&Nat` references; Tungsten passes by value.

## fibonacci_int (Tier 1, ADR 14.9.26c)

**Algorithm:** Naive recursive Fibonacci on the signed `Int` primitive — no Peano, no allocation.
**Recursion:** O(2^n) calls for fib(n), non-tail double recursion.
**Allocation:** None. `Int` is an `i64` in a register, the word `Nat` already is.
**Branch structure:** One signed compare (`n < 2`); every `+`/`-` is `llvm.sadd/ssub.with.overflow` plus a branch into the trap block, so the workload measures the overflow check on the hottest arithmetic shape.
**Input size:** fib(30) = 832040.
**Iteration:** 8 iterations (default), read from argv.
**Observable output:** Sum of `fib(30)` across all iterations. Default: 6656320.
**Known differences:** No Rust twin yet; **no speed-up over `Nat` is claimed** — the comparison is checked `Int` against the unchecked `Nat` word, never against Peano.

## collatz_int (Tier 1, ADR 14.9.26c)

**Algorithm:** Collatz sequence length on the signed `Int` primitive, tail-style with an accumulator.
**Recursion:** Depth = sequence length (111 for 27).
**Allocation:** None.
**Branch structure:** `n <= 1`, `n % 2 == 0`; each `/` and `%` carries the zero-divisor and `MIN / -1` guards, each `*` and `+` the overflow branch — four guards per step next to a `Nat` loop with none.
**Input size:** collatz(27) = 111 steps.
**Iteration:** 100000 iterations (default), read from argv.
**Observable output:** Sum of `collatz_length(27, 0)` across all iterations. Default: 11100000.
**Known differences:** No Rust twin yet; no speed-up over `Nat` claimed. Verified at 1000 iterations = 111000 on the host at close-out; wall-clock numbers are a manual run under `benchmarks/MANIFEST.md`, not recorded here.
