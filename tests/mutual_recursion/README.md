# Mutual Recursion Tests

Tests for Bug #2: Mutual Type Recursion Support

## Problem

Types must currently be defined before use, preventing mutually recursive patterns:

```tungsten
type Expr = ExprBlock(List<Stmt>)  // Error: Stmt not yet defined
type Stmt = StmtExpr(Expr)
```

## Test Annotation Format

Tests use `// @expect-error: EXXXX` to indicate expected compile failures:

```tungsten
// @expect-error: E0001
// This test expects a "type not found" error
type Foo = Bar  // Bar doesn't exist
```

Tests without `@expect-error` are expected to compile successfully once Bug #2 is fixed.

## Test Cases

| File | Description | Expected After Fix |
|------|-------------|-------------------|
| `basic_mutual.tg` | A references B, B references A | ✓ Compiles |
| `three_way.tg` | A → B → C → A cycle | ✓ Compiles |
| `expr_stmt.tg` | Practical AST pattern (Expr/Stmt ADTs) | ✓ Compiles |
| `with_generics.tg` | Mutual recursion with type parameters | ✓ Compiles |
| `undefined_in_mutual.tg` | References non-existent type | ✗ Clean error E0001 |

## Running Tests

```bash
cargo run -p tungsten_bootstrap -- check tests/mutual_recursion/basic_mutual.tg
```

## Success Criteria

- All non-`@expect-error` tests compile successfully
- `undefined_in_mutual.tg` produces a clean "unknown type" error (not an ICE)
