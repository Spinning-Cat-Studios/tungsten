// Benchmark: Naive recursive Fibonacci (Peano Nat — structurally equivalent)
//
// Rust baseline uses Box<Nat> Peano encoding to match Tungsten's allocation
// pattern. This is NOT idiomatic Rust — it mirrors Tungsten's representation.
// 8 iterations of fib(25) for ~200ms runtime.

use std::fmt;

#[derive(Clone)]
enum Nat {
    Zero,
    Succ(Box<Nat>),
}

fn from_u64(n: u64) -> Nat {
    if n == 0 {
        Nat::Zero
    } else {
        Nat::Succ(Box::new(from_u64(n - 1)))
    }
}

fn to_u64(n: &Nat) -> u64 {
    match n {
        Nat::Zero => 0,
        Nat::Succ(pred) => 1 + to_u64(pred),
    }
}

fn add(a: &Nat, b: &Nat) -> Nat {
    match a {
        Nat::Zero => b.clone(),
        Nat::Succ(pred) => Nat::Succ(Box::new(add(pred, b))),
    }
}

fn sub(a: &Nat, b: &Nat) -> Nat {
    match (a, b) {
        (_, Nat::Zero) => a.clone(),
        (Nat::Succ(a_pred), Nat::Succ(b_pred)) => sub(a_pred, b_pred),
        (Nat::Zero, _) => Nat::Zero,
    }
}

fn fib(n: &Nat) -> Nat {
    match n {
        Nat::Zero => Nat::Zero,
        Nat::Succ(pred) => match pred.as_ref() {
            Nat::Zero => from_u64(1),
            _ => {
                let one = from_u64(1);
                let two = from_u64(2);
                let a = fib(&sub(n, &one));
                let b = fib(&sub(n, &two));
                add(&a, &b)
            }
        },
    }
}

impl fmt::Display for Nat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", to_u64(self))
    }
}

fn run_once() -> u64 {
    let n = from_u64(25);
    to_u64(&fib(&n))
}

fn main() {
    let iters: u64 = std::env::args().nth(1).map_or(8, |s| s.parse().unwrap());
    let mut total: u64 = 0;
    for _ in 0..iters {
        total += run_once();
    }
    println!("{}", total);
}
