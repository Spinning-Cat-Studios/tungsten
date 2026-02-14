# Tungsten

**A dependently-typed language where proofs and programs are one.**

Tungsten is a self-hosted, dependently-typed functional language that combines the theorem proving power of Lean with the ergonomics of Rust. Write code, prove it correct, and compile it to native binaries — all in one language.

## Highlights

- **Self-Hosted Compiler**: The Tungsten compiler is written in Tungsten itself, with a verified triple-compile fixed point
- **Small Trusted Core**: ~1.5K LOC trusted kernel — if the core is correct, your proofs are sound
- **AI-Friendly Diagnostics**: Levenshtein-based "did you mean?" suggestions, contextual hints, and structured error spans designed for both humans and LLMs
- **Lean + Rust Flavor**: Familiar syntax for Rust developers, with dependent types and proofs inspired by Lean
- **Native Compilation**: LLVM 18 backend for compiling to native binaries
- **Incremental Compilation**: Build cache with dependency tracking for fast iteration

## Quick Start

Pre-built binaries are available for macOS (ARM64) and Linux (x86_64) on the [Releases](../../releases) page.

**macOS note:** The binaries are not code-signed. macOS will show a "cannot be verified" dialog on first run. To fix this:

```bash
xattr -d com.apple.quarantine tungsten libtungsten_core.dylib tungsten-bootstrap
```

You can verify download integrity with the `checksums.txt` file included in each release:

```bash
shasum -a 256 -c checksums.txt
```

**Linux note:** The Linux binary is x86_64 and requires glibc ≥ 2.39 (Ubuntu 24.04+).
On older distros or non-x86 hosts (including Apple Silicon Macs), use the included
Docker tooling:

```bash
# Build the Docker image (done automatically on first run)
./tungsten-docker.sh build

# Run tungsten commands inside the container
./tungsten-docker.sh run check myfile.tg
./tungsten-docker.sh run compile hello.tg -o hello
./tungsten-docker.sh run run examples/hello.tg

# Open an interactive shell
./tungsten-docker.sh shell
```

Type-checking (`tungsten check`) and interpreted execution (`tungsten run`) work without LLVM. Only `tungsten compile` requires LLVM 18.

### Building from Source

**Without LLVM** — supports `check`, `run`, and `eval` (no native compilation):

```bash
cargo build --release -p tungsten_bootstrap --no-default-features

./target/release/tungsten run examples/hello.tg         # → hello world
./target/release/tungsten check examples/arithmetic.tg
./target/release/tungsten eval "2 + 2"                  # → 4
```

**With LLVM 18** — adds the `compile` subcommand for native binaries (see [LLVM Setup](#llvm-setup-for-native-compilation)):

```bash
# macOS with Homebrew (use /path/to/llvm@18 on other platforms)
LLVM_SYS_180_PREFIX=$(brew --prefix llvm@18) cargo build --release

./target/release/tungsten compile examples/hello.tg -o hello
./hello                                                 # → hello world
```

### Using Make

```bash
# Interpreted execution (no LLVM needed)
make run FILE=examples/hello.tg

# Native compilation (requires LLVM 18)
make compile FILE=examples/hello.tg OUT=hello
./hello

# Or compile and run in one step
make compile-run FILE=examples/hello.tg

# Don't have LLVM? Use the dev container:
make devcontainer-up
make devcontainer-compile-run FILE=examples/hello.tg
```

## Examples

**Hello World** (`examples/hello.tg`):
```tungsten
fn main() -> String {
    "hello world"
}
```

**Arithmetic** (`examples/arithmetic.tg`):
```tungsten
fn main() -> Nat {
    20 + 20 + 2
}
```

**Boolean Logic** (`examples/logic.tg`):
```tungsten
fn my_not(b: Bool) -> Bool {
    if b { false } else { true }
}

fn my_and(a: Bool, b: Bool) -> Bool {
    if a { b } else { false }
}
```

**Algebraic Data Types:**
```tungsten
type Option<T> = None | Some(T)
type List<T> = Nil | Cons(T, List<T>)
type Result<T, E> = Ok(T) | Err(E)

fn unwrap_or<T>(opt: Option<T>, default: T) -> T {
    match opt {
        None => default,
        Some(x) => x,
    }
}
```

**Record Types:**
```tungsten
type Point = { x: Nat, y: Nat }

fn origin() -> Point {
    { x: 0, y: 0 }
}
```

**Modules:**
```tungsten
mod utils;      // loads utils.tg or utils/mod.tg
mod math;

use utils::helper;
use math::{add, sub};
```

See the `examples/` directory for more, including proofs (`proof.tg`, `proofs_boolean.tg`, `proofs_natural.tg`).

## AI-Friendly Diagnostics

Tungsten provides rich, contextual error messages designed for both humans and AI assistants:

```
Error: type mismatch
   ┌─ src/main.tg:5:12
   │
 5 │     let x: String = 42;
   │            ^^^^^^   ^^ found Nat
   │            │
   │            expected String
   │
   = help: consider using `nat_to_string(42)` to convert
```

- **Contextual hints**: "expected because of return type annotation at line 3"
- **Did-you-mean suggestions**: Levenshtein-based typo detection for identifiers
- **Constructor suggestions**: When you use the wrong constructor, shows valid options
- **Structured spans**: Precise source locations with context

## What Works / What Doesn't

### ✅ Working

- **Self-hosted compiler** — the compiler compiles itself, with verified triple-compile fixed point
- **Type checking** — dependent types, generics, ADTs, records, pattern matching
- **Native compilation** — LLVM 18 backend produces native binaries
- **Proofs** — dependent types enable theorem proving (see `examples/proof.tg`)
- **AI-friendly diagnostics** — rich errors with Levenshtein suggestions and contextual hints
- **Rust-style modules** — `mod`, `use`, with incremental build cache
- **FFI** — `extern "C" fn` for calling C/Rust functions

### ⚠️ Known Limitations

- **Module resolution uses flattening** — full hierarchical resolution planned for v1.5
- **Self-hosted native compiler currently macOS (ARM64) only** — Linux/x86_64 ships the bootstrap (Rust-built) compiler, which is fully functional. Self-hosted native compilation on x86_64 is planned for v1.5 while target-aware alignment and ABI work is completed
- **Interpreter supports a reduced subset** — `tungsten run` reliably evaluates `Nat` arithmetic, `String` results, and simple functions, but does not yet reduce eliminators (`if`/`else`, `match`, record field access). Native compilation (`tungsten compile`) handles all features correctly. Fix planned for v1.5
- **`tungsten run` in macOS release binary** — the self-hosted binary shipped in the macOS archive always prints `()` due to a codegen bug with record field access. Use `tungsten compile` instead, which works correctly. Fix planned for v1.5
- **No borrow checker** — memory safety relies on immutability; mutable references require manual care
- **Proof tactics not yet exposed** — equality types exist in the core but surface syntax for tactics is planned for v1.5

### ❌ Not Yet (v1.5 / v2.0)

- LSP / IDE support
- Formatter
- `let mut` syntax
- REPL
- Lean4 transpiler
- Tactic language

## Project Structure

```
tungsten/
├── tungsten_core/     # Trusted kernel (Rust) — type checker & evaluator
├── bootstrap/         # Bootstrap compiler (Rust) — lexer, parser, elaborator
├── tungsten_codegen/  # LLVM codegen crate (Rust)
├── src/compiler/      # Self-hosted compiler (Tungsten)
├── stdlib/            # Standard library (Tungsten)
├── examples/          # Example programs and proofs
└── tests/             # Test suite including golden tests
```

## Architecture

Tungsten uses a **small trusted kernel** architecture:

- **`tungsten_core`** (~1.5K LOC Rust): The trusted computing base. Contains the type checker and evaluator. If this code is correct, Tungsten is sound.

- **`bootstrap`** (Rust): The bootstrap compiler. Parses surface syntax and elaborates it to Core terms. Bugs here can't compromise soundness — they can only cause compilation to fail.

- **`src/compiler/`** (Tungsten): The self-hosted compiler. Written in Tungsten, compiled by the bootstrap compiler, then compiles itself. The triple-compile fixed point has been verified.

- **`tungsten_codegen`** (Rust): LLVM 18 code generation via inkwell.

## Commands

| Command | Description |
|---------|-------------|
| `tungsten <file>` | Run a file (evaluate `main()`) |
| `tungsten run <file>` | Same as above |
| `tungsten check <file>` | Type-check without running |
| `tungsten eval <expr>` | Evaluate an expression |
| `tungsten compile <file>` | Compile to native code (requires LLVM 18) |
| `tungsten clean` | Clear the build cache |
| `tungsten cache stats` | Show cache statistics |

### Native Compilation

```bash
# Compile to native binary
tungsten compile examples/hello.tg -o hello
./hello

# Emit LLVM IR instead of compiling
tungsten compile examples/hello.tg --emit-llvm
```

**Note:** Native compilation requires LLVM 18. Type-checking (`tungsten check`) and interpreted execution (`tungsten run`) do **not** require LLVM.

## Development

```bash
# Build (interpreter only, no LLVM required)
cargo build --release -p tungsten_bootstrap --no-default-features

# Build with native compilation support (requires LLVM 18)
# macOS with Homebrew (use /path/to/llvm@18 on other platforms)
LLVM_SYS_180_PREFIX=$(brew --prefix llvm@18) cargo build --release

# Run tests
make test

# Run golden tests
make check-golden
```

### LLVM Setup (for native compilation)

**macOS (Homebrew):**
```bash
brew install llvm@18
export LLVM_SYS_180_PREFIX=$(brew --prefix llvm@18)
```

**Linux (apt):**
```bash
apt install llvm-18 llvm-18-dev
export LLVM_SYS_180_PREFIX=/usr/lib/llvm-18
```

Then build with codegen:
```bash
cargo build --release
```

If building without LLVM, use `--no-default-features` to disable the codegen feature.

## Stability

Tungsten 1.0 means the compiler pipeline is self-hosted and usable. The language is still evolving; breaking changes may occur until 2.0. Semver applies to tooling releases, not language surface stability.

## Support

Tungsten is a personal research project; support is best-effort. Bug reports with minimal reproductions are welcome via GitHub Issues.

## Contributing

Bug reports, questions, and documentation fixes are welcome. Pull requests are not being accepted for v1.0 — see [CONTRIBUTING.md](CONTRIBUTING.md) for details and the plan for v1.5.

## License

MIT
