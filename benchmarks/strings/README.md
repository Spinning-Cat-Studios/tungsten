# String Benchmarks

## string_concat

**Algorithm:** Recursive string concatenation. Builds a string of `44 × N` characters by repeatedly prepending a 44-character base string.
**Recursion:** O(N) recursive calls. Non-tail-recursive (concat before recursive return).
**Allocation:** Each concatenation allocates a new string. Rust uses `format!("{}{}", s, repeat(s, n-1))`, Tungsten uses `tg_string_concat` FFI.
**Branch structure:** Base case (n=0) returns empty string; recursive case concatenates.
**Input size:** 15000 repetitions (default), producing a 660000-character string.
**Iteration:** Single iteration (the repetition count IS the workload parameter).
**Observable output:** Length of the final concatenated string. Default: 660000.
**Known differences:** Rust `format!` uses the standard allocator; Tungsten `tg_string_concat` uses the runtime's string allocator. Both produce identical-length strings. Output is the string length (not the string itself) to keep `.expected` files manageable.
