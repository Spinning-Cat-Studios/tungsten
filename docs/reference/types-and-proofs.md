# Types and Proofs

This document covers Tungsten's type system and its use for theorem proving.

## Core idea

Tungsten is built on the Curry-Howard correspondence: **types are propositions, and programs are proofs**. When you write a function with a particular type signature, you are constructing a proof that the proposition (the type) is true.

The trusted kernel (~1.5K LOC) type-checks all core terms. If the kernel is correct, every proof that Tungsten accepts is valid.

## Built-in types

### Nat

Natural numbers. Supports `+`, `-`, `*`, `/`, `%`, and comparison operators.

```tungsten
fn factorial(n: Nat) -> Nat {
    if n == 0 { 1 } else { n * factorial(n - 1) }
}
```

### Bool

Boolean values `true` and `false`. Supports `&&`, `||`, `==`.

```tungsten
fn is_even(n: Nat) -> Bool {
    n % 2 == 0
}
```

### String

String values with concatenation via `++`.

```tungsten
fn greet(name: String) -> String {
    "Hello, " ++ name ++ "!"
}
```

### Unit

The unit type, written `()`. Used when a function has no meaningful return value.

## Algebraic data types

### Sum types (enums)

Define types with multiple constructors using `|`:

```tungsten
type Option<T> = None | Some(T)

type Result<T, E> = Ok(T) | Err(E)

type List<T> = Nil | Cons(T, List<T>)

type Tree<T> = Leaf | Node(T, Tree<T>, Tree<T>)
```

Constructors are pattern-matched with `match`:

```tungsten
fn unwrap_or<T>(opt: Option<T>, default: T) -> T {
    match opt {
        None() => default,
        Some(x) => x,
    }
}
```

Note: nullary constructors require `()` in patterns (e.g., `None()`, `Leaf()`, `Nil()`).

### Record types

Define types with named fields:

```tungsten
type Point = { x: Nat, y: Nat }

type Span = { start: Nat, end: Nat, file: String }
```

Construct records with `{ field: value }` syntax. Access fields with dot notation:

```tungsten
fn make_point(x: Nat, y: Nat) -> Point {
    { x: x, y: y }
}

fn get_x(p: Point) -> Nat {
    p.x
}
```

### Pair types

A common pattern using a single-constructor ADT:

```tungsten
type Pair<A, B> = MkPair(A, B)

fn fst(p: Pair<Nat, Bool>) -> Nat {
    match p {
        MkPair(a, b) => a,
    }
}
```

## Generics

Functions and types can be parameterised by types:

```tungsten
fn id<T>(x: T) -> T { x }

fn const_fn<A, B>(x: A, y: B) -> A { x }

fn compose<A, B, C>(f: B -> C, g: A -> B, x: A) -> C {
    f(g(x))
}
```

Type arguments are usually inferred from context. You can provide them explicitly when needed:

```tungsten
let x = id(42);          // T inferred as Nat
let y = id::<Nat>(42);   // T provided explicitly
```

## Function types

Functions are first-class values. The type `A -> B` describes a function from `A` to `B`:

```tungsten
fn apply<A, B>(f: A -> B, x: A) -> B {
    f(x)
}

fn main() -> Nat {
    let succ = |x: Nat| x + 1;
    apply(succ, 5)    // → 6
}
```

Lambda expressions use `|params| body` syntax:

```tungsten
let double = |x: Nat| x * 2;
let add = |x: Nat, y: Nat| x + y;
```

## Pattern matching

The `match` expression destructures values by their constructors:

```tungsten
fn describe(opt: Option<Nat>) -> String {
    match opt {
        None() => "nothing",
        Some(x) => "something",
    }
}
```

Patterns can bind variables and use wildcards:

```tungsten
fn head_or_zero(list: List<Nat>) -> Nat {
    match list {
        Nil() => 0,
        Cons(x, _) => x,
    }
}
```

The compiler checks exhaustiveness — all constructors must be covered, or a wildcard `_` must be present.

**Current limitation:** Pattern matching is limited to one level of destructuring. Nested patterns like `Cons(x, Nil())` are planned for v1.5. Currently, you must nest `match` expressions manually:

```tungsten
// v1.0: manual nesting required
fn is_singleton(list: List<Nat>) -> Bool {
    match list {
        Nil() => false,
        Cons(x, rest) => match rest {
            Nil() => true,
            Cons(_, _) => false,
        },
    }
}
```

## Dependent types

In Tungsten, types can depend on values. This is the foundation of the proof system.

The core calculus supports:

- **Equality types:** `Eq<T, a, b>` — the proposition that `a` equals `b` at type `T`
- **Reflexivity:** `refl` — proof that `x = x`
- **Substitution:** if `a = b` and `P(a)`, then `P(b)`

### Theorem declarations

The `theorem` keyword declares a proposition to be proven:

```tungsten
theorem zero_is_nat() -> Nat {
    0
}
```

### Proofs by exhaustive testing

Since the full equality type surface syntax is still evolving, proofs in v1.0 are typically done by exhaustive computational verification. For types like `Bool` (two values), this is complete:

```tungsten
fn not(x: Bool) -> Bool {
    if x { false } else { true }
}

// Verify: not(not(x)) == x for all x : Bool
fn test_double_negation() -> Bool {
    not(not(true)) == true && not(not(false)) == false
}
```

### Proofs with Peano naturals

For natural number properties, Tungsten examples verify properties computationally for small values:

```tungsten
type Peano = Zero | Succ(Peano)

fn add(m: Peano, n: Peano) -> Peano {
    match m {
        Zero => n,
        Succ(pred) => Succ(add(pred, n)),
    }
}

fn nat_eq(m: Peano, n: Peano) -> Bool {
    match m {
        Zero => match n {
            Zero => true,
            Succ(_) => false,
        },
        Succ(m_pred) => match n {
            Zero => false,
            Succ(n_pred) => nat_eq(m_pred, n_pred),
        },
    }
}

// Verify: add is commutative (for small values)
fn test_add_commutative() -> Bool {
    nat_eq(add(one(), two()), add(two(), one())) &&
    nat_eq(add(two(), three()), add(three(), two()))
}
```

See `examples/proofs_boolean.tg` and `examples/proofs_natural.tg` for complete examples covering De Morgan's laws, distributivity, associativity, and ordering properties.

## What's coming

The proof surface is expanding in future versions:

- **v1.5:** Equality type surface syntax, basic tactics (`refl`, `sym`, `trans`, `cong`), test framework with `#[test]` annotations
- **v2.0:** Full tactic language (`simp`, `induction`, `rewrite`, `ring`, `omega`), decision procedures, refinement types
