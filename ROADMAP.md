# Tungsten Roadmap

This roadmap outlines the planned evolution of Tungsten across major versions. Dates are not committed — Tungsten is a personal research project and features ship when they're ready.

## v1.0 — Self-Hosted Compiler (current)

Tungsten 1.0 establishes the language as a working, self-hosted dependently-typed proof language.

**Completed:**

- Self-hosted compiler written in Tungsten, compiled by the Rust bootstrap
- Verified triple-compile fixed point (compiler compiles itself to a functionally identical binary across generations)
- Small trusted kernel (~1.5K LOC) — if the core is correct, your proofs are sound
- Dependent types, generics, algebraic data types, records, pattern matching
- LLVM 18 backend for native compilation
- Interpreted execution (no LLVM required for `tungsten check` and `tungsten run`)
- Rust-style module system with `mod` and `use`
- FFI via `extern "C" fn`
- AI-friendly diagnostics with Levenshtein suggestions and contextual hints
- Standard library with `Nat`, `Bool`, `Option`, `List`, `Result`, string operations
- Proof examples demonstrating boolean algebra and natural number properties

**Known limitations in v1.0:**

- Module resolution uses flattening (all modules merged before elaboration)
- Self-hosted native compiler currently supported on macOS (ARM64) — Linux/x86_64 self-hosting requires target-aware alignment and ABI work (bootstrap compiler runs on both platforms)
- Interpreter (`tungsten run`) does not reduce eliminators — `if`/`else`, `match`, and record field access return `()` instead of correct values. Type-checking and native compilation are unaffected
- Self-hosted binary's `tungsten run` prints `()` for all programs due to a codegen bug with record field projection. Use `tungsten compile` instead, which works correctly
- `return` keyword does not interrupt control flow — it type-checks the value but execution continues (effectively a no-op)
- Two diagnostic gaps: E0007 (duplicate import) and E0016 (private item access) not emitted by self-hosted compiler
- `pub` keyword rejected in standalone files — files using `pub fn` (needed for module imports) cannot be type-checked standalone with `tungsten check`; a consequence of module flattening
- Pattern matching limited to one level of destructuring (no nested patterns)

---

## v1.5 — Language Completeness

v1.5 focuses on filling in features that were deliberately simplified during bootstrap. With the compiler written in Tungsten itself, iteration is faster.

**Planned features:**

- **Linux/x86_64 self-host support** — add target-aware alignment and ABI support for the self-compiled native compiler on x86_64 Linux (reference platform: ARM64 macOS)
- **Interpreter eliminator fix** — reduce `if`/`else`, `match`, and record field access in the evaluator so `tungsten run` produces correct results
- **Self-hosted `tungsten run` codegen fix** — fix record field projection codegen bug that causes the self-hosted binary to print `()` for all interpreted programs
- **Remove module flattening** — proper per-module compilation, prerequisite for incremental builds
- **Incremental build cache** — content-addressed caching for the self-hosted compiler
- **Early return** — true control flow interruption (currently `return` is a no-op)
- **Nested pattern matching** — `Cons(x, Nil())` instead of manual nested `match`
- **Record spread syntax** — `{ ...record, field: new_value }`
- **`let mut` syntax** — sugar over reference cells
- **`?` operator** — early return on error for `Result`/`Option` (requires early return)
- **Diagnostic improvements** — error cascade prevention (reduce 30x amplification), missing E0007/E0016 emissions, multi-file span rendering for cross-file errors, import chain tracing, "did you mean?" suggestions for typos
- **Or-patterns** — `Null | False | Zero => ...`
- **Visibility enhancements** — per-constructor and per-field visibility, re-exports
- **Test framework** — `#[test]` annotations, `tungsten test` CLI command
- **Self-hosted LLVM IR emitter** — replace bootstrap codegen dependency
- **Typed union codegen** — fix SROA-safe sum type representation (SROA = Scalar Replacement of Aggregates)
- **Deterministic LLVM IR emission** — achieve byte-identical `.ll` files from `tungsten2` and `tungsten3`, proving the entire compiler pipeline is deterministic (full binary equivalence deferred to v2.0)

---

## v2.0 — Production-Ready (long-term vision)

v2.0 aims to make Tungsten a production-ready proof assistant and systems language. This is a vision document — features may be promoted to v1.5 or deferred further based on research.

**Planned features:**

- **Borrow checker** — Rust-style ownership and borrowing for memory safety
- **Tactic language** — `simp`, `induction`, `rewrite`, `ring`, `omega`, `auto`
- **Algebraic effects** — first-class effects for controlled side effects
- **LSP server** — go-to-definition, hover, diagnostics, code actions
- **REPL** — interactive evaluation and proof exploration
- **Package manager** — Cargo-like dependency management
- **Binary-reproducible triple compile** — upgrade from IR equivalence (v1.5) to byte-identical binaries via linker/environment determinism
- **Lean4 transpiler** — transpile proofs to Lean4 for independent verification
- **Type classes / traits** — overloading and abstraction
- **Refinement types** — types with predicates
- **Row types** — extensible records
- **Standard library expansion** — collections, IO, concurrency, proof library

---

## Design Principles

These principles guide feature design across all versions:

1. **Small trusted core** — new features elaborate to existing core constructs when possible
2. **AI-first** — rich error messages, structured diagnostics, documentation
3. **Proofs are programs** — Curry-Howard correspondence throughout
4. **Familiar syntax** — Rust/Lean-inspired, not novel for novelty's sake
5. **Incremental adoption** — features should be opt-in where possible
