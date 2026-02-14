# Tungsten Overview

## What is Tungsten?

Tungsten is a dependently-typed functional programming language that combines the theorem proving power of Lean with the ergonomics of Rust. It lets you write code, prove it correct, and compile it to native binaries — all in one language.

Tungsten is a personal research project exploring how dependent types, proof checking, and practical systems programming can coexist in a single language with familiar syntax.

## Goals

- **Proofs are programs.** Tungsten uses the Curry-Howard correspondence: types are propositions, programs are proofs. If your code type-checks, its logical content is verified.
- **Small trusted core.** The type checker and evaluator are ~1.5K lines of Rust. If this kernel is correct, all proofs checked by Tungsten are sound. The rest of the compiler (parser, elaborator, codegen) cannot compromise soundness — bugs there can only cause compilation to fail.
- **Familiar syntax.** Tungsten borrows syntax from Rust (`fn`, `let`, `match`, `mod`, `use`, `type`) and ideas from Lean (dependent types, `theorem`, propositions). The goal is a language that feels approachable to Rust developers while offering theorem proving capabilities.
- **AI-friendly diagnostics.** Error messages are designed for both humans and LLMs: structured spans, contextual hints, Levenshtein-based "did you mean?" suggestions, and constructor alternatives.
- **Self-hosting.** The Tungsten compiler is written in Tungsten itself. The triple-compile fixed point has been verified: the compiler compiles itself, and the output compiles itself again to produce a functionally identical binary.

## Architecture

```
                        ┌──────────────────────────┐
    .tg source ────────▶│  Parser (surface syntax) │
                        └───────────┬──────────────┘
                                    │ AST
                        ┌───────────▼──────────────┐
                        │  Elaborator              │
                        │  (type inference,        │
                        │   proof checking)        │
                        └───────────┬──────────────┘
                                    │ Core terms
                  ┌─────────────────┼──────────────────┐
                  │                 │                  │
          ┌───────▼───────┐  ┌──────▼───────┐  ┌───────▼──────┐
          │ Type Checker  │  │ Interpreter  │  │ LLVM Codegen │
          │ (tungsten_    │  │ (tungsten    │  │ (tungsten    │
          │  core)        │  │  run)        │  │  compile)    │
          │ ~1.5K LOC     │  │              │  │ requires     │
          │ TRUSTED       │  │              │  │ LLVM 18      │
          └───────────────┘  └──────────────┘  └──────────────┘
```

### Key components

- **`tungsten_core`** (Rust, ~1.5K LOC) — The trusted computing base. Contains the type checker and evaluator for the core calculus. This is the only code that matters for soundness.

- **`bootstrap/`** (Rust) — The bootstrap compiler. Parses Tungsten surface syntax and elaborates it into core terms. Cannot compromise soundness.

- **`src/compiler/`** (Tungsten) — The self-hosted compiler. Written in Tungsten, compiled by the bootstrap compiler. Replaces the bootstrap for day-to-day development.

- **`tungsten_codegen/`** (Rust) — LLVM 18 code generation via inkwell. Translates core terms to LLVM IR for native compilation.

- **`stdlib/`** — Standard library providing core types and operations.

- **`examples/`** — Sample programs and proofs demonstrating language features.

## How it works

### Type checking

Tungsten's core calculus is a dependent type theory where types can depend on values. The type checker verifies that terms have the types they claim to have, including checking proofs.

```tungsten
// Types can contain values
fn replicate<T>(n: Nat, x: T) -> List<T> { ... }

// Propositions are types; proofs are programs
theorem zero_plus_n(n: Nat) : 0 + n = n { refl }
```

When you run `tungsten check file.tg`, the elaborator translates your surface syntax into core terms and the trusted kernel verifies them. No code is generated — this is pure verification.

### Execution

Tungsten programs can be executed in two ways:

1. **Interpreted** (`tungsten run file.tg`) — The elaborator produces core terms, and the interpreter evaluates them directly. No LLVM required.

2. **Compiled** (`tungsten compile file.tg -o binary`) — Core terms are lowered to LLVM IR and compiled to a native binary. Requires LLVM 18.

Both modes require the program to type-check first. You cannot run ill-typed code.

### Self-hosting

The Tungsten compiler is written in Tungsten:

1. The **bootstrap compiler** (Rust) compiles the Tungsten source → `tungsten1`
2. `tungsten1` compiles the same source → `tungsten2`
3. `tungsten2` compiles the same source → `tungsten3`
4. `tungsten2` and `tungsten3` are functionally identical (fixed point)

This triple-compile verification confirms that the self-hosted compiler faithfully reproduces itself. Functional equivalence means identical behavior on all inputs — exit codes, output, and diagnostics match exactly. Binary equivalence (byte-identical outputs) is a goal for v1.5.

## Current status

Tungsten 1.0 is the first public release. The compiler pipeline is complete and self-hosted, but the language is still evolving. See [ROADMAP.md](../ROADMAP.md) for what's planned next, and the README for what works and what doesn't.

Semver applies to tooling releases, not language surface stability. Breaking changes may occur until v2.0.
