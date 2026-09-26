# Changelog

## v2.0.0-alpha.3 — 2026-09-27


### Added


#### Language
- Signed integers: an `Int` type beside `Nat`, with context-typed literals (`-7`, or `42` where an `Int` is expected), negation, truncating division, overflow and division-by-zero traps, and the built-in conversions `to_int` / `from_int`
- `match` on numbers (`Nat` and `Int`): literal, negative-literal, or-of-literal (`1 | 2`), variable and wildcard patterns with `if` guards; exactly one unguarded catch-all is required
- Strict positivity check (E0061): a type whose recursive occurrence appears to the left of an arrow is rejected, including when inherited through another type
- Structural termination checking (E0062): every recursive function must pass a strictly smaller piece of a parameter at each recursive call; `#[decreasing(arg)]` names the parameter when more than one shrinks
- `#[partial]` opts a function out of termination checking; a proof may not depend on a partial definition, directly or through a wrapper (E0063)
- A recursive type nested under a generic parameter (`Node(List<Tree>)`) is reported as E0064 at the `match` instead of hanging the compiler
- (macOS) The shipped self-hosted compiler enforces the positivity and termination checks and understands `#[partial]`, as the Linux build does

#### Runtime
- `StringBuilder`: six runtime functions, reached through `extern "C"`, for building a string from many pieces in linear time
- `TUNGSTEN_ARENA=off|bump[:chunk_mib]`: an opt-in bump-arena allocator for natively compiled programs; memory is returned at exit, and benchmarks run 1.3–1.7× faster

#### Tooling
- `tungsten doctor check module name-collisions <file>` names both modules when two define the same name (the E0016 case), and runs on files that fail to compile
- `tungsten doctor check sorry-sites <file>` lists every `sorry` hole by definition, and says whether you wrote it or the compiler generated it
- `tungsten test --require-tests` fails when no runnable tests are found
- `tungsten test` bounds evaluation, so a runaway test is reported instead of hanging; a type mismatch whose two types print identically now shows where they differ
- The golden-test runner ships with the repository, so `make golden` works from a fresh clone

### Changed

- Termination checking is a hard gate for every definition by default (`--termination all`); `--termination proofs|report` and `TUNGSTEN_TERMINATION` relax it for bisecting
- `tungsten explain error` accepts the code printed in a diagnostic (`E0010`), not only the internal kind name
- A broken type becomes an error type that suppresses follow-on errors; distinct errors are no longer discarded, and folded duplicates are counted
- A broken type definition is reported once, at the definition, instead of at every place that builds a value of it
- `check` reports every broken module in one run, your own mistake first, instead of stopping at the first file
- `tungsten test` fails a test that executed no assertion (`ASSERTED NOTHING` / `DID NOT FINISH`) instead of reporting it as passing
- Errors formerly reported under one generic code get their own codes when they are the user's, and are labelled as internal compiler faults when they are not
- With the arena off (the default), allocation takes a direct path with no extra call
- (macOS) The shipped compiler rejects the same non-positive types as the Linux build, using one shared checker
- (macOS) The shipped compiler rejects a proof that relies on a `#[partial]` definition
- The repository's make files carry only the build, test and compiler-correctness targets a contributor needs; internal release and maintenance tooling is no longer included
- CI invokes only targets the shipped make files define

### Fixed

- Tail-recursive functions returning a record or a payload-carrying variant are tail-call optimised again instead of overflowing the stack
- Tail-call optimisation covers more struct-shaped parameters and results
- Tail-recursive functions taking large or nested struct parameters run in constant stack
- Generic functions that call non-generic top-level functions no longer fail to compile with "global not found"
- Compiled programs no longer call the wrong module's function when two imported modules define the same name
- Some `match` expressions with unreachable arms no longer fail to compile with an internal "shrinking cast" error
- `run`, `check` and `test` given a non-canonical path (`a/../b.tg`) no longer report a spurious E0030 or poison the cache
- `run` and `test` on a warm cache no longer report "no main function found" or "no tests found"
- Programs with mutually recursive data types no longer take minutes and gigabytes of memory to compile
- A generic type stored in a record field or constructor payload no longer causes a bogus "expected `X<T>`, found `X<T>`"
- A global that refers to itself during evaluation gets a clear diagnostic instead of a stack overflow
- Recursive generic functions in compiled programs no longer allocate a closure on every call
- Error underlines and column numbers are correct after non-ASCII characters earlier in the file
- `examples/proofs_natural.tg` passes the termination check again (`#[decreasing(m)]` on its two-argument comparisons)
- (macOS) `tungsten test` finds the same tests as the Linux build and fails when none are found
- (macOS) The generated `tungsten test` harness no longer collides with names in the program under test (spurious E0016)
- (macOS) `match` on a single-constructor type is accepted
- (macOS) Named record construction and spread assign each value to the named field
- (macOS) When a helper program cannot start, the compiler says so instead of printing a bare exit number
- (macOS) `tungsten test` names the file and explains when the built test program fails to start
- `make` no longer refuses every target on a clone without the internal tool submodules
- `make check-tco-gate` finds its skip list on a fresh clone
- Documentation and code comments no longer point at files that are not part of the repository

## v2.0.0-alpha.2 — 2026-09-27


### Added


#### Language
- Signed integers: an `Int` type beside `Nat`, with context-typed literals (`-7`, or `42` where an `Int` is expected), negation, truncating division, overflow and division-by-zero traps, and the built-in conversions `to_int` / `from_int`
- `match` on numbers (`Nat` and `Int`): literal, negative-literal, or-of-literal (`1 | 2`), variable and wildcard patterns with `if` guards; exactly one unguarded catch-all is required
- Strict positivity check (E0061): a type whose recursive occurrence appears to the left of an arrow is rejected, including when inherited through another type
- Structural termination checking (E0062): every recursive function must pass a strictly smaller piece of a parameter at each recursive call; `#[decreasing(arg)]` names the parameter when more than one shrinks
- `#[partial]` opts a function out of termination checking; a proof may not depend on a partial definition, directly or through a wrapper (E0063)
- A recursive type nested under a generic parameter (`Node(List<Tree>)`) is reported as E0064 at the `match` instead of hanging the compiler
- (macOS) The shipped self-hosted compiler enforces the positivity and termination checks and understands `#[partial]`, as the Linux build does

#### Runtime
- `StringBuilder`: six runtime functions, reached through `extern "C"`, for building a string from many pieces in linear time
- `TUNGSTEN_ARENA=off|bump[:chunk_mib]`: an opt-in bump-arena allocator for natively compiled programs; memory is returned at exit, and benchmarks run 1.3–1.7× faster

#### Tooling
- `tungsten doctor check module name-collisions <file>` names both modules when two define the same name (the E0016 case), and runs on files that fail to compile
- `tungsten doctor check sorry-sites <file>` lists every `sorry` hole by definition, and says whether you wrote it or the compiler generated it
- `tungsten test --require-tests` fails when no runnable tests are found
- `tungsten test` bounds evaluation, so a runaway test is reported instead of hanging; a type mismatch whose two types print identically now shows where they differ
- The golden-test runner ships with the repository, so `make golden` works from a fresh clone

### Changed

- Termination checking is a hard gate for every definition by default (`--termination all`); `--termination proofs|report` and `TUNGSTEN_TERMINATION` relax it for bisecting
- `tungsten explain error` accepts the code printed in a diagnostic (`E0010`), not only the internal kind name
- A broken type becomes an error type that suppresses follow-on errors; distinct errors are no longer discarded, and folded duplicates are counted
- A broken type definition is reported once, at the definition, instead of at every place that builds a value of it
- `check` reports every broken module in one run, your own mistake first, instead of stopping at the first file
- `tungsten test` fails a test that executed no assertion (`ASSERTED NOTHING` / `DID NOT FINISH`) instead of reporting it as passing
- Errors formerly reported under one generic code get their own codes when they are the user's, and are labelled as internal compiler faults when they are not
- With the arena off (the default), allocation takes a direct path with no extra call
- (macOS) The shipped compiler rejects the same non-positive types as the Linux build, using one shared checker
- (macOS) The shipped compiler rejects a proof that relies on a `#[partial]` definition
- The repository's make files carry only the build, test and compiler-correctness targets a contributor needs; internal release and maintenance tooling is no longer included
- CI invokes only targets the shipped make files define

### Fixed

- Tail-recursive functions returning a record or a payload-carrying variant are tail-call optimised again instead of overflowing the stack
- Tail-call optimisation covers more struct-shaped parameters and results
- Tail-recursive functions taking large or nested struct parameters run in constant stack
- Generic functions that call non-generic top-level functions no longer fail to compile with "global not found"
- Compiled programs no longer call the wrong module's function when two imported modules define the same name
- Some `match` expressions with unreachable arms no longer fail to compile with an internal "shrinking cast" error
- `run`, `check` and `test` given a non-canonical path (`a/../b.tg`) no longer report a spurious E0030 or poison the cache
- `run` and `test` on a warm cache no longer report "no main function found" or "no tests found"
- Programs with mutually recursive data types no longer take minutes and gigabytes of memory to compile
- A generic type stored in a record field or constructor payload no longer causes a bogus "expected `X<T>`, found `X<T>`"
- A global that refers to itself during evaluation gets a clear diagnostic instead of a stack overflow
- Recursive generic functions in compiled programs no longer allocate a closure on every call
- Error underlines and column numbers are correct after non-ASCII characters earlier in the file
- `examples/proofs_natural.tg` passes the termination check again (`#[decreasing(m)]` on its two-argument comparisons)
- (macOS) `tungsten test` finds the same tests as the Linux build and fails when none are found
- (macOS) The generated `tungsten test` harness no longer collides with names in the program under test (spurious E0016)
- (macOS) `match` on a single-constructor type is accepted
- (macOS) Named record construction and spread assign each value to the named field
- (macOS) When a helper program cannot start, the compiler says so instead of printing a bare exit number
- (macOS) `tungsten test` names the file and explains when the built test program fails to start
- `make` no longer refuses every target on a clone without the internal tool submodules
- `make check-tco-gate` finds its skip list on a fresh clone
- Documentation and code comments no longer point at files that are not part of the repository

## v2.0.0-alpha.1 — 2026-09-26


### Added


#### Language
- Signed integers: an `Int` type beside `Nat`, with context-typed literals (`-7`, or `42` where an `Int` is expected), negation, truncating division, overflow and division-by-zero traps, and the built-in conversions `to_int` / `from_int`
- `match` on numbers (`Nat` and `Int`): literal, negative-literal, or-of-literal (`1 | 2`), variable and wildcard patterns with `if` guards; exactly one unguarded catch-all is required
- Strict positivity check (E0061): a type whose recursive occurrence appears to the left of an arrow is rejected, including when inherited through another type
- Structural termination checking (E0062): every recursive function must pass a strictly smaller piece of a parameter at each recursive call; `#[decreasing(arg)]` names the parameter when more than one shrinks
- `#[partial]` opts a function out of termination checking; a proof may not depend on a partial definition, directly or through a wrapper (E0063)
- A recursive type nested under a generic parameter (`Node(List<Tree>)`) is reported as E0064 at the `match` instead of hanging the compiler
- (macOS) The shipped self-hosted compiler enforces the positivity and termination checks and understands `#[partial]`, as the Linux build does

#### Runtime
- `StringBuilder`: six runtime functions, reached through `extern "C"`, for building a string from many pieces in linear time
- `TUNGSTEN_ARENA=off|bump[:chunk_mib]`: an opt-in bump-arena allocator for natively compiled programs; memory is returned at exit, and benchmarks run 1.3–1.7× faster

#### Tooling
- `tungsten doctor check module name-collisions <file>` names both modules when two define the same name (the E0016 case), and runs on files that fail to compile
- `tungsten doctor check sorry-sites <file>` lists every `sorry` hole by definition, and says whether you wrote it or the compiler generated it
- `tungsten test --require-tests` fails when no runnable tests are found
- `tungsten test` bounds evaluation, so a runaway test is reported instead of hanging; a type mismatch whose two types print identically now shows where they differ
- The golden-test runner ships with the repository, so `make golden` works from a fresh clone

### Changed

- Termination checking is a hard gate for every definition by default (`--termination all`); `--termination proofs|report` and `TUNGSTEN_TERMINATION` relax it for bisecting
- `tungsten explain error` accepts the code printed in a diagnostic (`E0010`), not only the internal kind name
- A broken type becomes an error type that suppresses follow-on errors; distinct errors are no longer discarded, and folded duplicates are counted
- A broken type definition is reported once, at the definition, instead of at every place that builds a value of it
- `check` reports every broken module in one run, your own mistake first, instead of stopping at the first file
- `tungsten test` fails a test that executed no assertion (`ASSERTED NOTHING` / `DID NOT FINISH`) instead of reporting it as passing
- Errors formerly reported under one generic code get their own codes when they are the user's, and are labelled as internal compiler faults when they are not
- With the arena off (the default), allocation takes a direct path with no extra call
- (macOS) The shipped compiler rejects the same non-positive types as the Linux build, using one shared checker
- (macOS) The shipped compiler rejects a proof that relies on a `#[partial]` definition
- The repository's make files carry only the build, test and compiler-correctness targets a contributor needs; internal release and maintenance tooling is no longer included
- CI invokes only targets the shipped make files define

### Fixed

- Tail-recursive functions returning a record or a payload-carrying variant are tail-call optimised again instead of overflowing the stack
- Tail-call optimisation covers more struct-shaped parameters and results
- Tail-recursive functions taking large or nested struct parameters run in constant stack
- Generic functions that call non-generic top-level functions no longer fail to compile with "global not found"
- Compiled programs no longer call the wrong module's function when two imported modules define the same name
- Some `match` expressions with unreachable arms no longer fail to compile with an internal "shrinking cast" error
- `run`, `check` and `test` given a non-canonical path (`a/../b.tg`) no longer report a spurious E0030 or poison the cache
- `run` and `test` on a warm cache no longer report "no main function found" or "no tests found"
- Programs with mutually recursive data types no longer take minutes and gigabytes of memory to compile
- A generic type stored in a record field or constructor payload no longer causes a bogus "expected `X<T>`, found `X<T>`"
- A global that refers to itself during evaluation gets a clear diagnostic instead of a stack overflow
- Recursive generic functions in compiled programs no longer allocate a closure on every call
- Error underlines and column numbers are correct after non-ASCII characters earlier in the file
- `examples/proofs_natural.tg` passes the termination check again (`#[decreasing(m)]` on its two-argument comparisons)
- (macOS) `tungsten test` finds the same tests as the Linux build and fails when none are found
- (macOS) The generated `tungsten test` harness no longer collides with names in the program under test (spurious E0016)
- (macOS) `match` on a single-constructor type is accepted
- (macOS) Named record construction and spread assign each value to the named field
- (macOS) When a helper program cannot start, the compiler says so instead of printing a bare exit number
- (macOS) `tungsten test` names the file and explains when the built test program fails to start
- `make` no longer refuses every target on a clone without the internal tool submodules
- `make check-tco-gate` finds its skip list on a fresh clone
- Documentation and code comments no longer point at files that are not part of the repository

## v2.0.0-alpha.0 — 2026-09-25


### Added

#### Language
- Signed integers: an `Int` type beside `Nat`, with context-typed literals (`-7`, or `42` where an `Int` is expected), negation, truncating division, overflow and division-by-zero traps, and the built-in conversions `to_int` / `from_int`
- `match` on numbers (`Nat` and `Int`): literal, negative-literal, or-of-literal (`1 | 2`), variable and wildcard patterns with `if` guards; exactly one unguarded catch-all is required
- Strict positivity check (E0061): a type whose recursive occurrence appears to the left of an arrow is rejected, including when inherited through another type
- Structural termination checking (E0062): every recursive function must pass a strictly smaller piece of a parameter at each recursive call; `#[decreasing(arg)]` names the parameter when more than one shrinks
- `#[partial]` opts a function out of termination checking; a proof may not depend on a partial definition, directly or through a wrapper (E0063)
- A recursive type nested under a generic parameter (`Node(List<Tree>)`) is reported as E0064 at the `match` instead of hanging the compiler
- (macOS) The shipped self-hosted compiler enforces the positivity and termination checks and understands `#[partial]`, as the Linux build does

#### Runtime
- `StringBuilder`: six runtime functions, reached through `extern "C"`, for building a string from many pieces in linear time
- `TUNGSTEN_ARENA=off|bump[:chunk_mib]`: an opt-in bump-arena allocator for natively compiled programs; memory is returned at exit, and benchmarks run 1.3–1.7× faster

#### Tooling
- `tungsten doctor check module name-collisions <file>` names both modules when two define the same name (the E0016 case), and runs on files that fail to compile
- `tungsten doctor check sorry-sites <file>` lists every `sorry` hole by definition, and says whether you wrote it or the compiler generated it
- `tungsten test --require-tests` fails when no runnable tests are found
- `tungsten test` bounds evaluation, so a runaway test is reported instead of hanging; a type mismatch whose two types print identically now shows where they differ

### Changed

- Termination checking is a hard gate for every definition by default (`--termination all`); `--termination proofs|report` and `TUNGSTEN_TERMINATION` relax it for bisecting
- `tungsten explain error` accepts the code printed in a diagnostic (`E0010`), not only the internal kind name
- A broken type becomes an error type that suppresses follow-on errors; distinct errors are no longer discarded, and folded duplicates are counted
- A broken type definition is reported once, at the definition, instead of at every place that builds a value of it
- `check` reports every broken module in one run, your own mistake first, instead of stopping at the first file
- `tungsten test` fails a test that executed no assertion (`ASSERTED NOTHING` / `DID NOT FINISH`) instead of reporting it as passing
- Errors formerly reported under one generic code get their own codes when they are the user's, and are labelled as internal compiler faults when they are not
- With the arena off (the default), allocation takes a direct path with no extra call
- (macOS) The shipped compiler rejects the same non-positive types as the Linux build, using one shared checker
- (macOS) The shipped compiler rejects a proof that relies on a `#[partial]` definition

### Fixed

- Tail-recursive functions returning a record or a payload-carrying variant are tail-call optimised again instead of overflowing the stack
- Tail-call optimisation covers more struct-shaped parameters and results
- Tail-recursive functions taking large or nested struct parameters run in constant stack
- Generic functions that call non-generic top-level functions no longer fail to compile with "global not found"
- Compiled programs no longer call the wrong module's function when two imported modules define the same name
- Some `match` expressions with unreachable arms no longer fail to compile with an internal "shrinking cast" error
- `run`, `check` and `test` given a non-canonical path (`a/../b.tg`) no longer report a spurious E0030 or poison the cache
- `run` and `test` on a warm cache no longer report "no main function found" or "no tests found"
- Programs with mutually recursive data types no longer take minutes and gigabytes of memory to compile
- A generic type stored in a record field or constructor payload no longer causes a bogus "expected `X<T>`, found `X<T>`"
- A global that refers to itself during evaluation gets a clear diagnostic instead of a stack overflow
- Recursive generic functions in compiled programs no longer allocate a closure on every call
- Error underlines and column numbers are correct after non-ASCII characters earlier in the file
- `examples/proofs_natural.tg` passes the termination check again (`#[decreasing(m)]` on its two-argument comparisons)
- (macOS) `tungsten test` finds the same tests as the Linux build and fails when none are found
- (macOS) The generated `tungsten test` harness no longer collides with names in the program under test (spurious E0016)
- (macOS) `match` on a single-constructor type is accepted
- (macOS) Named record construction and spread assign each value to the named field
- (macOS) When a helper program cannot start, the compiler says so instead of printing a bare exit number
- (macOS) `tungsten test` names the file and explains when the built test program fails to start

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
