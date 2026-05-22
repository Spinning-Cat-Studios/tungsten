# Tungsten Roadmap

This roadmap outlines the planned evolution of Tungsten across major versions. Dates are not committed — Tungsten is a personal research project and features ship when they're ready.

## v1.0 — Self-Hosted Compiler

Tungsten 1.0 established the language as a working, self-hosted dependently-typed proof language.

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

---

## v1.5 — Language Completeness (current)

v1.5 focused on compiler architecture, performance, and language ergonomics. All v1.0 known limitations have been resolved.

**Completed:**

- **Linux/x86_64 self-host support** — target-aware alignment and ABI for self-compiled native compiler on both ARM64 macOS and x86_64 Linux
- **Interpreter and codegen fixes** — `tungsten run` produces correct results; self-hosted binary works correctly
- **Per-module compilation** — module flattening removed; three-phase per-module elaboration pipeline (Phase A / A.5 / B)
- **Elaboration caching** — three-tier content-addressed cache; warm-cache self-compile under 500ms
- **Parallel codegen** — per-function codegen units with in-process parallel compilation
- **Static linking** — single binary output, no `LD_LIBRARY_PATH` required
- **Early return** — `return expr` with bottom type semantics
- **`?` operator** — early return on error for `Result`/`Option`
- **`let`-`else` syntax** — refutable pattern binding with diverging else branch
- **`if let` expressions** — with chain support
- **`try` blocks** — create `Result` values without function boundaries
- **Nested pattern matching** — `Cons(Pair(a, b), rest)` and nested tuples in constructors
- **Record spread syntax** — `{ ...base, field: new_value }`
- **Named record constructors** — `TypeName { field: value, ... }`
- **Generic type aliases** — `type Alias<T> = ConcreteType<T>`
- **Visibility enforcement** — per-constructor, per-field, and `pub use` re-export capping
- **Diagnostic improvements** — error cascade prevention, multi-file spans, contextual hints, 50+ diagnostic commands
- **Test framework** — `tungsten test` with discovery, filtering, and compile-time assertions
- **Self-hosted LLVM IR emitter** — `.tg`-based codegen (M4–M9 complete)
- **ABI-safe sum types** — SROA-safe tagged union representation
- **Deterministic IR emission** — byte-identical `.ll` from tungsten2/tungsten3
- **Musttail TCO** — proper tail-call optimization, removes stack trampoline
- **Escape analysis** — non-escaping closures and ADT values stack-allocated
- **Propositional equality** — `Eq<A, x, y>` type with `Refl` constructor and motive-driven dependent elimination
- **Natural number induction** — `natind` eliminator for primitive recursion with compile-time motive checking
- **Benchmarking suite** — 7 benchmarks comparing Tungsten vs Rust

---

## v2.0 — Production-Ready

v2.0 focuses on making Tungsten a production-ready platform: allocation performance, proof automation foundations, developer tooling, and ecosystem scaffolding.

**Planned features:**

- **Allocation optimizations** — unbox small ADTs as tagged machine words, arena allocation for compiler phases, string interning
- **Tactic language foundations** — `simp`, `induction`, `rewrite`, `cases`, and arithmetic decision procedures
- **DWARF debug info** — source-level debugging in GDB/LLDB for `.tg` programs
- **Diagnostic infrastructure** — structured JSON error output, fix-it suggestions, runtime `.tg` stack traces
- **LSP design + minimal implementation** — go-to-definition, hover, inline diagnostics
- **Documentation tooling** — generated module/function docs for humans and agents
- **Package manifest** — `tungsten.toml` with local path dependencies and workspace support
- **Native core library** — rewrite `tungsten_core` runtime in Tungsten itself
- **Parallel elaboration** — multi-threaded per-module type-checking
- **`let mut` syntax** — sugar over reference cells
- **Or-patterns** — `Null | False | Zero => ...`
- **Standard library expansion** — collections, string builder, proof library foundations
- **Termination checking** — structural recursion verification for proof soundness
- **Universe hierarchy** — stratified `Type 0 : Type 1 : ...` to prevent Girard's paradox
- **Cross-platform release builds** — CI release matrix for macOS, Linux, and Windows with distributable binaries

---

## v2.1 — Ecosystem & Adoption (long-term vision)

v2.1 focuses on ecosystem maturity and enabling others to use Tungsten productively. This is a vision document — features require significant design work and real-world usage feedback from v2.0.

**Planned features:**

- **Community contributions** — structured PR workflow, contributor guidelines, commit-preserving release pipeline
- **Borrow checker** — Rust-style ownership and borrowing for memory safety
- **Type classes / traits** — overloading, abstraction, and method syntax
- **Algebraic effects** — first-class effects for controlled side effects
- **Crypto primitives** — type-safe hashing, encryption, signatures with linear key management
- **Networking (sockets, TCP/IP)** — socket lifecycle as a type-level state machine; TCP, UDP, DNS
- **Provable network models** — session types and effect-typed failure reasoning for distributed protocols
- **Full LSP** — completions, rename, code actions, error recovery, incremental re-elaboration
- **REPL** — interactive evaluation and proof exploration
- **Lean4 transpiler** — transpile proofs to Lean4 for independent verification
- **Binary-reproducible triple compile** — byte-identical binaries via linker/environment determinism
- **Refinement types** — types with predicates
- **Inductive families** — indexed types (length-indexed vectors, well-typed ASTs)
- **Proof irrelevance** — Prop universe with compile-time proof erasure
- **Row types** — extensible records
- **Per-definition incremental elaboration** — Salsa-style query-based re-checking
- **Full package registry** — version resolution, publishing, git dependencies

---

## v2.2 — Proof Language Maturity (long-term vision)

v2.2 closes the gap with mature proof assistants like Lean4, focusing on extensible proof automation and mathematical foundations.

**Planned features:**

- **Metaprogramming framework** — custom tactics, elaboration extensions, and linters written in Tungsten itself
- **Notation extensions** — user-defined operators, notation macros, and syntax categories
- **Quotient types** — equivalence classes as types (integers, rationals, modular arithmetic)
- **Coercions** — automatic type coercions via `Coe<A, B>` typeclass
- **Proof library foundations** — algebraic hierarchy, order theory, common lemmas

---

## Design Principles

These principles guide feature design across all versions:

1. **Small trusted core** — new features elaborate to existing core constructs when possible
2. **AI-first** — rich error messages, structured diagnostics, documentation
3. **Proofs are programs** — Curry-Howard correspondence throughout
4. **Familiar syntax** — Rust/Lean-inspired, not novel for novelty's sake
5. **Incremental adoption** — features should be opt-in where possible
