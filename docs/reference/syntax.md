# Tungsten Syntax Reference

Quick reference for the Tungsten language grammar and constructs.
For the type system and proof patterns, see [types-and-proofs.md](types-and-proofs.md).

---

## Comments

```tungsten
// Line comment.

/// Doc comment (attached to the next declaration).
/// Supports multiple lines.
```

---

## Declarations

### Functions

```tungsten
fn add(a: Nat, b: Nat) -> Nat {
    a + b
}
```

Functions are expressions — the last expression in the body is the return value.
There is no `return` keyword.

Generic functions:

```tungsten
fn identity<T>(x: T) -> T {
    x
}
```

Public functions:

```tungsten
pub fn my_api(x: Nat) -> Nat {
    x + 1
}
```

### Theorems

`theorem` is syntactically identical to `fn` but declares that the function's
type represents a proposition and its body a proof.

```tungsten
theorem bool_identity() -> Bool {
    let t = not(not(true)) == true;
    let f = not(not(false)) == false;
    t && f
}
```

### Type Declarations

#### Sum types (algebraic data types)

```tungsten
type Option<T> =
    | None
    | Some(T)
```

Single-constructor types don't need the leading `|`:

```tungsten
type Pair<A, B> = MkPair(A, B)
```

#### Record types

```tungsten
type FileParseResult = {
    items: ItemList,
    errors: ParseErrorList,
}
```

#### Type aliases

```tungsten
pub type Char = Nat
pub type SourceEntry = (String, String)
```

### Extern Functions (FFI)

```tungsten
extern "C" fn tg_string_length(s: String) -> Nat

pub extern "C" fn tg_cstr_eq(a: Nat, b: Nat) -> Bool
```

Extern declarations have no body. They bind to C-ABI symbols resolved at link
time.

### Let Bindings

```tungsten
let x = 42;
let name: String = "hello";
```

Type annotations are optional when the type can be inferred.

#### Tuple destructuring

```tungsten
let (a, b) = make_pair();
let (x, (y, z)) = make_nested();   // nested destructuring
let (_, b) = make_pair();           // wildcard to ignore a field
```

---

## Modules

### Declaring sub-modules

```tungsten
mod lexer;
mod parser;
mod elab;
```

Each `mod name;` expects either a `name.tg` file or a `name/mod.tg` directory
module.

### Imports

```tungsten
use lexer::token::Token;
use driver::{parse_args, dispatch_command, process_exit};
```

Paths are absolute from the crate root. `super` is not used.

### Visibility

Declarations are private by default. Mark them `pub` to export from the current
module:

```tungsten
pub fn public_function() -> Nat { 1 }
pub type PublicType = { value: Nat }
pub extern "C" fn public_extern() -> Unit
```

---

## Types

### Built-in types

| Type     | Description                            |
|----------|----------------------------------------|
| `Nat`    | Natural numbers (unsigned integers)    |
| `Bool`   | `true` / `false`                       |
| `String` | UTF-8 text                             |
| `Unit`   | The unit type (single value `()`)      |

### Tuples

```tungsten
fn make_pair() -> (Nat, Nat) {
    (1, 2)
}

fn make_triple() -> (Nat, Nat, Nat) {
    (10, 20, 30)
}
```

### Function types

```tungsten
fn apply(f: (Nat) -> Nat, x: Nat) -> Nat {
    f(x)
}
```

### Generic type arguments

When the compiler can't infer a type argument, supply it explicitly with the
turbofish syntax:

```tungsten
let x = identity::<Nat>(42);
```

---

## Expressions

### Literals

```tungsten
42          // Nat
true        // Bool
false       // Bool
"hello"     // String
()          // Unit
(1, 2)      // Tuple
```

### Operators

| Operator | Description              | Example              |
|----------|--------------------------|----------------------|
| `+`      | Addition                 | `a + b`              |
| `==`     | Equality                 | `x == y`             |
| `!=`     | Inequality               | `x != y`             |
| `<`      | Less than                | `a < b`              |
| `>`      | Greater than             | `a > b`              |
| `<=`     | Less than or equal       | `a <= b`             |
| `>=`     | Greater than or equal    | `a >= b`             |
| `&&`     | Logical AND              | `p && q`             |
| `\|\|`   | Logical OR               | `p \|\| q`           |
| `++`     | String concatenation     | `"a" ++ "b"`         |

### If / else

`if` is an expression that returns a value:

```tungsten
let label = if count == 1 { "item" } else { "items" };
```

Both branches must have the same type. There is no `else if` — nest another
`if` inside the `else` branch.

### Match

Pattern match on constructors:

```tungsten
fn describe(opt: Option<Nat>) -> String {
    match opt {
        None => "nothing",
        Some(n) => "got a number",
    }
}
```

Wildcard patterns:

```tungsten
fn span_start(s: Span) -> Nat {
    match s {
        MkSpan(start, _, _) => start,
    }
}
```

Catch-all:

```tungsten
match value {
    SpecificCase => 1,
    _ => 0,
}
```

**v1.0 limitation:** nested patterns (e.g. `Some(Some(x))`) are not yet
supported. Flatten with multiple `match` expressions instead.

### Lambdas

```tungsten
let inc = |x: Nat| x + 1;
let add = |a: Nat, b: Nat| a + b;
```

### Function calls

```tungsten
let result = add(1, 2);
```

### Field access (records)

```tungsten
let name = entry.name;
let pos = cursor.position;
```

### Record construction

```tungsten
let span: Span = { start: 0, end: 10, file: filename };
```

### String concatenation

```tungsten
let msg = "Expected " ++ nat_to_string(n) ++ " arguments";
```

There is no string interpolation. Build strings with `++` and conversion
functions like `nat_to_string`.

### Blocks

Curly braces introduce a block. The last expression is the block's value:

```tungsten
let result = {
    let (x, y) = make_pair();
    x + y
};
```

---

## Patterns

Used in `match` arms and `let` destructuring.

| Pattern            | Description                             |
|--------------------|-----------------------------------------|
| `Constructor`      | Nullary constructor match               |
| `Constructor(a,b)` | Constructor with field bindings          |
| `_`                | Wildcard (ignore value)                 |
| `name`             | Variable binding                        |
| `(a, b)`           | Tuple destructuring (in `let`)          |

Constructors without arguments are matched by name alone (e.g. `None`, `Zero`).
When called as values (not in patterns), nullary constructors may need `()` —
for example `None()` vs `None` depending on context.

---

## Program Entry Point

Every executable needs a `main` function. Its return value is printed to
stdout; the process always exits with code 0. Any type with a printable
representation (`Nat`, `String`, `Bool`, etc.) may be returned:

```tungsten
fn main() -> String {
    "hello world"
}
```

```tungsten
fn main() -> Nat {
    42
}
```

---

## Semicolons

`let` bindings inside function bodies end with `;`. The final expression in a
block is **not** followed by a semicolon. Match arms are separated by `,`.

```tungsten
fn example() -> Nat {
    let x = 1;          // semicolon after let binding
    let y = 2;          // semicolon after let binding
    x + y               // no semicolon — this is the return value
}
```

---

## v1.0 Known Limitations

- No nested patterns in `match` — flatten into sequential matches.
- No early return (`return` keyword).
- No `else if` — use nested `if` inside `else`.
- No `-`, `*`, `/`, `%` as built-in infix operators — use library functions.
- No string interpolation.
- No spread/rest syntax for records.
- Modules are flattened to a single namespace after elaboration.
- See [ROADMAP.md](../../ROADMAP.md) for what's planned in v1.5 and v2.0.
