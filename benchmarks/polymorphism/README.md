# Polymorphism Benchmarks

## generic_map

**Algorithm:** Apply a polymorphic `map` function over a 5-element Peano list, doubling each element. Measures generic instantiation and polymorphic dispatch overhead.
**Recursion:** `map` recurses through the list. Non-tail-recursive.
**Allocation:** Each mapped element produces a new list node and new Peano number. Rust uses `Box<List<T>>`, Tungsten uses implicit heap allocation with monomorphized generics.
**Branch structure:** Match on Nil/Cons for list, match on Zero/Succ for Peano arithmetic.
**Input size:** 5-element list, each element doubled (Peano `add(n, n)`). Result = sum of doubled elements = 100 per call.
**Iteration:** 10000 iterations (default), read from argv.
**Observable output:** Sum of results across all iterations. Default: 1000000.
**Known differences:** Rust uses monomorphized generics (same as Tungsten). Both produce identical generic instantiation patterns.
