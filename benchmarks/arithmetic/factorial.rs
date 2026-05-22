// Benchmark: Recursive factorial (Peano Nat — structurally equivalent)
//
// Note: Input capped at 7. Peano mul/add recurse O(result) deep;
// factorial(9)+ overflows the stack (~362K Succ nodes).
// 500 iterations of factorial(7) for ~350ms runtime.

#[derive(Clone)]
enum Nat {
    Zero,
    Succ(Box<Nat>),
}

fn from_u64(n: u64) -> Nat {
    if n == 0 { Nat::Zero } else { Nat::Succ(Box::new(from_u64(n - 1))) }
}

fn to_u64(n: &Nat) -> u64 {
    match n { Nat::Zero => 0, Nat::Succ(p) => 1 + to_u64(p) }
}

fn sub(a: &Nat, b: &Nat) -> Nat {
    match (a, b) {
        (_, Nat::Zero) => a.clone(),
        (Nat::Succ(ap), Nat::Succ(bp)) => sub(ap, bp),
        (Nat::Zero, _) => Nat::Zero,
    }
}

fn mul(a: &Nat, b: &Nat) -> Nat {
    match a {
        Nat::Zero => Nat::Zero,
        Nat::Succ(p) => add(b, &mul(p, b)),
    }
}

fn add(a: &Nat, b: &Nat) -> Nat {
    match a {
        Nat::Zero => b.clone(),
        Nat::Succ(p) => Nat::Succ(Box::new(add(p, b))),
    }
}

fn factorial(n: &Nat) -> Nat {
    match n {
        Nat::Zero => from_u64(1),
        _ => {
            let one = from_u64(1);
            mul(n, &factorial(&sub(n, &one)))
        }
    }
}

fn run_once() -> u64 {
    let n = from_u64(7);
    to_u64(&factorial(&n))
}

fn main() {
    let iters: u64 = std::env::args().nth(1).map_or(500, |s| s.parse().unwrap());
    let mut total: u64 = 0;
    for _ in 0..iters {
        total += run_once();
    }
    println!("{}", total);
}
