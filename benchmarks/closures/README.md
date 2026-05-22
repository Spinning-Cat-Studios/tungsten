# Closure Benchmarks

## closure_chain

**Algorithm:** Chain of 360 composed closures, each adding a small constant. Measures closure allocation and application overhead.
**Recursion:** `apply_chain` recurses through a linked list of 360 closures. Non-tail-recursive.
**Allocation:** Each closure captures its constant and the next closure in the chain. Rust uses `Box<dyn Fn>`, Tungsten uses implicit closure allocation.
**Branch structure:** Match on Nil/Cons for the closure list.
**Input size:** 360 closures applied per call, result = 360.
**Iteration:** 5000 iterations (default), read from argv.
**Observable output:** Sum of `apply_chain(chain, 0)` across all iterations. Default: 1800000.
**Known differences:** Rust uses trait objects (`Box<dyn Fn(u64) -> u64>`) for closures; Tungsten uses native closure representation (function pointer + environment pointer).
