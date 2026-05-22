# Changelog

## v1.5.0 — 2026-05-22


### Added

#### Language Features
- Early return (`return expr`) with bottom type (`⊥`) semantics
- `?` operator for `Result` and `Option` — desugars to match + early return
- `let`-`else` syntax — `let P = expr else { diverge }` for refutable pattern binding
- `if let` expressions with chain support (`if let Ok(x) = a && let Ok(y) = b { ... }`)
- `try` blocks — create `Result` values without requiring function boundaries
- Nested constructor patterns — `match xs { Cons(Pair(a, b), rest) => ... }`
- Named record constructors — `TypeName { field: value, ... }` in synth mode
- Record spread syntax — `{ ...base, field: new_value }` functional record update
- Generic type aliases — `type ParseResult<T> = Result<(T, Cursor), ParseError>`
- Nested tuple patterns in constructor arguments — `Ok((tok, cur))` matching
- Import aliasing — `use foo::Bar as Alias` renaming in import declarations
- Propositional equality type — `Eq<A, x, y>` with `Refl` constructor for type-safe equality proofs
- `natind` eliminator — primitive recursion on natural numbers with motive-driven type checking
- Motive type checking — eliminators (`natind`, `J`) infer and check motive types for dependent elimination

#### Compilation & Performance
- Per-function codegen units with in-process parallel compilation via `std::thread::scope`
- In-process LLVM object emission — eliminates `.ll` → `llc` → `.o` round-trip
- Elaboration caching (three tiers: signature-only, full CoreDef body, compressed background writes)
- Warm-cache self-compile under 500ms (down from ~32 minutes cold)
- Static linking of `libtungsten_core` — single binary, no `LD_LIBRARY_PATH` required
- Musttail tail-call optimization — removes 64MB stack trampoline
- Basic escape analysis — non-escaping closures and ADT values stack-allocated
- Uncurried calling convention — direct multi-argument entry points for known-arity functions
- String concat FFI extraction with owned-left `realloc` fast path for dead temporaries
- Tail-recursive list operations — accumulator-passing rewrites prevent stack overflow in `tungsten1`

#### Module System
- Three-phase per-module elaboration pipeline (Phase A / A.5 / B)
- Per-module import resolution with cross-module type and constructor stubs
- Single-owner monomorphization — each generic instantiation emitted exactly once
- Visibility enforcement: per-constructor, per-field, and `pub use` re-export capping

#### Diagnostics & Tooling
- Multi-file diagnostic spans with secondary labels and elaboration trace
- `tungsten doctor suggest-tools` — pattern-matching error → diagnostic command recommendations
- Diagnostic sidecar process with LMDB-backed experience store for tool effectiveness tracking
- Compiler-embedded diagnostic hints (contextual suggestions in error output)
- `tungsten test` command with test discovery, `--filter`, `--module`, `--check-only`
- `expect_type(expr, "T")` — cost-3 compile-time type assertion (no codegen needed)
- `expect_error(expr, "E0001")` — cost-3 compile-time error code assertion
- `tungsten diff types`, `diff core`, `diff abi`, `diff ir` — structural comparison tools
- `tungsten info type-encoding`, `info adt`, `info constructors`, `info cir sites`
- `tungsten doctor check-phase-invariants`, `check-fold-consistency`, `check-normalization-consistency`
- `tungsten cache clean` / `cache status` — elaboration cache management
- Chrome tracing profiling via `--features codegen,profile`
- `tungsten commands --tree` — hierarchical command discovery
- Benchmarking suite (7 benchmarks: Tungsten vs Rust) under `benchmarks/`
- IR determinism verification — byte-identical `.ll` output from tungsten2/tungsten3
- `tungsten doctor check type forall-resolution` — detect inner foralls blocking type extraction
- `tungsten diff l1-l2-check` — compare L1 and `tungsten1` check results on same source
- Benchmark runner tool with evidence bundles, equivalence verification, and deep analysis mode
- Performance attribution profiling — LLVM IR structural comparison for benchmark explanations
- x86_64 devcontainer — QEMU-based cross-architecture testing for self-compiled binaries
- Publish/promote tool in Rust — tree-filtered commit replay with staging release verification

#### Self-Hosting
- L3 self-host typecheck parity — `tungsten1 check` passes all 1962 L2 definitions with 0 errors
- Self-hosted `.tg` LLVM IR text emitter (1,393 lines across 13 files)
- Milestones M4–M9 complete: closures, sums/case, full self-compile capability
- CIR (Codegen IR) with capture list population via free-variable analysis

### Changed

- Module elaboration architecture: single-pass combined AST → three-phase per-module pipeline
- Codegen granularity: single monolithic `.ll` → per-function codegen units in `target/ll/`
- Sum type representation: opaque `[N x i8]` → `{ i32, [N x i8] }` tagged union (ABI-safe)
- Recursive ADT encoding: single μ-binder → nested μ-binders for mutual recursion (Tarjan SCC)
- CLI namespace reorganization: flat commands → hierarchical (`info type`, `info codegen`, `info module`, `doctor check type`, etc.)
- Makefile: monolithic 680-line file → modular `.mk` splits (core, usage, compiler, native, devcontainer, diagnostics, profiling, publishing, quality)
- Parser: ~30 bespoke `ParseResult*` ADTs → standard `Result<(T, Cursor), ParseError>` with `?`

### Fixed

- Interpreter eliminator: `tungsten run` now returns correct values for `if`/`else`, `match`, and record projection
- Self-hosted `run` codegen: `tungsten1 run` correctly projects record fields (Fst/Snd chain fix)
- Error cascade: failed function bodies no longer invalidate signatures — error count drops from 527 to ~18
- ARM64 ABI: multi-variant ADT struct register decomposition no longer corrupts payloads
- x86_64 self-host: self-compiled binary passes all 10 examples on x86_64 Linux
- Stack overflow in double self-compile: `filter_trivia_acc` rewritten iteratively (was overflowing 8MB stack)
- TyVar escape: 303 monomorphic definitions with free TyVars → 0 (mutual recursion encoding fix)
- Match arm type inference: multi-field constructor patterns now elaborate in check mode (not infer)
- Glob re-export duplicates: same-definition deduplication prevents spurious E0106 errors
- ADT constructor exports: constructors registered as importable value-level names
- `pub use` re-exports: visibility and path-qualified submodule imports resolved
- Nested directory module re-export ordering bug fixed
- Wildcard tuple projection: `let (_, n) = pair` now correctly projects `snd` (not `fst`)
- Generic type elaboration: duplicate constructor registration inflating variant counts
- Mono discovery: stdlib generic functions called from user code now correctly monomorphized
- L3 module ordering sensitivity: module splits no longer trigger false E0999 errors in `tungsten1`
- L3 inner forall instantiation: polymorphic constructors in `Result<(List<T>, Cursor), E>` patterns resolve correctly
- Nested constructor+tuple pattern codegen: `tungsten1` now binds variables from `Ok((a, b))` patterns

### Removed

- 64MB pthread stack trampoline (replaced by musttail TCO)
- Module flattening pass (replaced by per-module elaboration)
- `build_combined_source_file` (replaced by three-phase pipeline)
- ~30 bespoke `ParseResult*` ADT types (replaced by standard `Result`)

## v1.0.0 — 2026-02-15


### Added

- Self-hosted compiler: Tungsten compiles itself to native code via LLVM 18
- Triple-compile fixed point: compiler reproduces itself identically across three compilation passes
- Dependently-typed core with ~1,500 lines of trusted kernel code
- Native compilation (bootstrap toolchain) targeting macOS (arm64) and Linux (x86_64) via `tungsten compile`
- Type checking without LLVM via `tungsten check`
- Interpreter mode via `tungsten run`
- Standard library with core types (`Nat`, `Bool`, `String`, `List`, `Option`, `Result`, `Pair`, `Ordering`)
- Proof surface: propositional equality, boolean proofs, natural number proofs, and rewriting
- Pattern matching with exhaustiveness and unreachability checking
- Generic types with type-level computation
- Record types with named fields
- Tuple types with let-destructuring
- String operations and concatenation
- Module system (`mod`/`use`) with multi-file projects (flattening in v1.0)
- Golden test suite for compiler output verification
- CI pipeline: bootstrap build, LLVM matrix builds, integration tests, self-host verification (L2–L4)
- Tag-triggered release workflow producing macOS and Linux binaries
- Documentation: language overview, syntax reference, types and proofs reference
- Project governance: MIT license, contribution policy, code of conduct, security policy, issue templates
