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
