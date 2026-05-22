# Data Structure Benchmarks

## list_ops

**Algorithm:** Build a Peano-encoded list of 50 elements, then sum all elements. Measures linked-list construction and traversal.
**Recursion:** `build_list` and `sum_list` both recurse through the list. Non-tail-recursive.
**Allocation:** Each `Cons` cell allocates one heap node containing a Peano number and a tail pointer. Rust uses `Box<List>`, Tungsten uses implicit heap allocation.
**Branch structure:** Match on Nil/Cons for list operations, match on Zero/Succ for Peano arithmetic.
**Input size:** List of 50 elements, each a Peano number. Sum = 50 × 201 = 10100 per call (sum of 1..50 converted to Peano, then back).
**Iteration:** 200 iterations (default), read from argv.
**Observable output:** Sum of `to_nat(sum_list(build_list(mk_50())))` across all iterations. Default: 2020000.
**Known differences:** Rust uses `&List` references for traversal; Tungsten passes by value.
