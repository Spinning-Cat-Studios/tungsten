// Benchmark: List operations (Peano Nat — structurally equivalent)
//
// Rust baseline uses heap-allocated cons cells to match Tungsten's allocation.
// Uses ownership-passing (move semantics) to mirror Tungsten's value model.
// 200 iterations for ~200ms runtime.

#[derive(Clone)]
enum Nat {
    Zero,
    Succ(Box<Nat>),
}

enum List {
    Nil,
    Cons(Nat, Box<List>),
}

fn from_u64(n: u64) -> Nat {
    if n == 0 { Nat::Zero } else { Nat::Succ(Box::new(from_u64(n - 1))) }
}

fn to_u64(n: &Nat) -> u64 {
    match n { Nat::Zero => 0, Nat::Succ(p) => 1 + to_u64(p) }
}

fn add(a: Nat, b: Nat) -> Nat {
    match a {
        Nat::Zero => b,
        Nat::Succ(p) => Nat::Succ(Box::new(add(*p, b))),
    }
}

fn range(n: u64) -> List {
    if n == 0 { List::Nil }
    else { List::Cons(from_u64(n), Box::new(range(n - 1))) }
}

fn sum(xs: List) -> Nat {
    match xs {
        List::Nil => Nat::Zero,
        List::Cons(h, t) => add(h, sum(*t)),
    }
}

fn map(f: fn(Nat) -> Nat, xs: List) -> List {
    match xs {
        List::Nil => List::Nil,
        List::Cons(h, t) => List::Cons(f(h), Box::new(map(f, *t))),
    }
}

fn reverse_acc(xs: List, acc: List) -> List {
    match xs {
        List::Nil => acc,
        List::Cons(h, t) => reverse_acc(*t, List::Cons(h, Box::new(acc))),
    }
}

fn reverse(xs: List) -> List {
    reverse_acc(xs, List::Nil)
}

fn double(x: Nat) -> Nat {
    let x2 = x.clone(); // Must clone — Tungsten shares implicitly
    add(x, x2)
}

fn run_once() -> u64 {
    let xs = range(100);
    let doubled = map(double, xs);
    let rev = reverse(doubled);
    to_u64(&sum(rev))
}

fn main() {
    let iters: u64 = std::env::args().nth(1).map_or(200, |s| s.parse().unwrap());
    let mut total: u64 = 0;
    for _ in 0..iters {
        total += run_once();
    }
    println!("{}", total);
}
