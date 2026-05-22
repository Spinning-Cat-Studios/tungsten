// Benchmark: Collatz sequence length (Peano Nat — structurally equivalent)
//
// Rust baseline uses Box<Nat> Peano encoding to match Tungsten's representation.
// 100 iterations of collatz(27) for ~500ms runtime.

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

fn is_even(n: &Nat) -> bool {
    match n {
        Nat::Zero => true,
        Nat::Succ(pred) => match pred.as_ref() {
            Nat::Zero => false,
            Nat::Succ(pred2) => is_even(pred2),
        },
    }
}

fn div2(n: &Nat) -> Nat {
    match n {
        Nat::Zero => Nat::Zero,
        Nat::Succ(pred) => match pred.as_ref() {
            Nat::Zero => Nat::Zero,
            Nat::Succ(pred2) => Nat::Succ(Box::new(div2(pred2))),
        },
    }
}

fn add(a: &Nat, b: &Nat) -> Nat {
    match a {
        Nat::Zero => b.clone(),
        Nat::Succ(pred) => Nat::Succ(Box::new(add(pred, b))),
    }
}

fn mul(a: &Nat, b: &Nat) -> Nat {
    match a {
        Nat::Zero => Nat::Zero,
        Nat::Succ(pred) => add(b, &mul(pred, b)),
    }
}

fn collatz_length(n: &Nat) -> Nat {
    match n {
        Nat::Zero => Nat::Zero,
        Nat::Succ(pred) => match pred.as_ref() {
            Nat::Zero => Nat::Zero, // n == 1
            _ => {
                let one = from_u64(1);
                let three = from_u64(3);
                if is_even(n) {
                    Nat::Succ(Box::new(collatz_length(&div2(n))))
                } else {
                    let next = add(&mul(&three, n), &one);
                    Nat::Succ(Box::new(collatz_length(&next)))
                }
            }
        },
    }
}

fn run_once() -> u64 {
    let n = from_u64(27);
    to_u64(&collatz_length(&n))
}

fn main() {
    let iters: u64 = std::env::args().nth(1).map_or(100, |s| s.parse().unwrap());
    let mut total: u64 = 0;
    for _ in 0..iters {
        total += run_once();
    }
    println!("{}", total);
}
