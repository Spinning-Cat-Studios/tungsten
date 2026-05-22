# Benchmark Compiler Flags Manifest

Record the following for every benchmark run. Results are only publishable when this manifest is complete.

## Rust

| Field | Value |
|-------|-------|
| `rustc --version` | |
| Flags | `-O` (= `-C opt-level=2`) |
| Target triple | |
| Edition | `2021` |

## Tungsten

| Field | Value |
|-------|-------|
| Tungsten version | |
| `llc --version` (LLVM) | |
| llc flags | `-O2` |
| Target triple | |
| Codegen backend | LLVM (external `llc`) |

## Environment

| Field | Value |
|-------|-------|
| OS | |
| CPU | |
| RAM | |
| Container | (if applicable) |

## Notes

- LLVM major-version mismatch between `rustc` and `llc` must be documented prominently.
- Both compilers should target the same triple.
- Results with differing optimization levels are not publishable without explicit caveat.
