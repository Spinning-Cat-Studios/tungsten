# Module Bug Regression Tests

Tests for Bug #1: Cross-Module Type Resolution

## Problem

When a submodule imports types from a sibling module, the elaborator confuses 
record types across module boundaries due to span-based module ownership heuristics.

## Test Annotation Format

Tests use `// @expect-error: EXXXX` to indicate expected compile failures:

```tungsten
// @expect-error: E9999
// Description of what's being tested
```

Tests without this annotation are expected to compile successfully.

## Test Cases

Note: Inline module blocks (`mod foo { ... }`) are not supported by the parser.
All tests use file-based modules (`mod foo;` with separate `.tg` files).

| Directory | Description | Status |
|-----------|-------------|--------|
| `sibling_import/` | Basic: child2 imports from child1 | Currently fails |
| `nested_sibling/` | Multi-file: parser::consumer imports from parser::list | Currently fails |
| `super_import/` | utils/inner imports via super::super::types | Currently fails |
| `triple_module/` | Three modules: a→c, b→a import chain | Currently fails |
| `whitespace_noise/` | use near comments/whitespace (span heuristic killer) | Currently fails |
| `lexer_parser_pattern/` | Mirrors actual self-hosted compiler layout | Currently fails |

## Running Tests

```bash
# Run individual test
cargo run -p tungsten_bootstrap -- check tests/module_bugs/sibling_import/main.tg

# Run multi-file test
cargo run -p tungsten_bootstrap -- check tests/module_bugs/nested_sibling/main.tg
```

## Success Criteria

All tests should compile without errors after Bug #1 is fixed.
