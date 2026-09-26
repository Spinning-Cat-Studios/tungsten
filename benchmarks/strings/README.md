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

## string_builder

**Algorithm:** The `string_concat` workload — the same 44-character piece, the same `N` — appended into one growable buffer (ADR 14.9.26a). The two Tungsten rows of one bundle are therefore the O(N²)-copy versus amortised-O(1)-append comparison over identical input.
**Recursion:** O(N) recursive calls, tail-recursive: the builder handle threads through as the accumulator.
**Allocation:** One buffer, grown by amortised doubling (`grow_policy`: `max(2·cap, len + add, 16)`). Rust uses `String::push_str`; Tungsten uses the `tg_string_builder_*` FFI, whose buffer comes from the same C allocator `tg_string_concat` uses.
**Branch structure:** Base case (n=0) returns the handle; recursive case pushes and recurses.
**Input size:** 15000 repetitions (default), producing a 660000-character string.
**Iteration:** Single iteration (the repetition count IS the workload parameter).
**Observable output:** Byte length of the built string, read through `tg_string_builder_len` before `to_string` consumes the builder. Default: 660000.
**Known differences:** Rust's `String` growth policy is the standard library's, not `grow_policy`; both are amortised doubling, so the copy counts agree to within a constant.
