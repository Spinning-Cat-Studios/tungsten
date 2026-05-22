// Benchmark: Polymorphic list operations (structurally equivalent)
//
// Rust baseline uses monomorphized generics over Box-allocated lists.
// 10000 iterations for ~200ms runtime.

#[derive(Clone)]
enum Nat {
    Zero,
    Succ(Box<Nat>),
}

enum List<T> {
    Nil,
    Cons(T, Box<List<T>>),
}

fn from_u64(n: u64) -> Nat {
    if n == 0 { Nat::Zero } else { Nat::Succ(Box::new(from_u64(n - 1))) }
}

fn to_u64(n: &Nat) -> u64 {
    match n { Nat::Zero => 0, Nat::Succ(p) => 1 + to_u64(p) }
}

fn add(a: &Nat, b: &Nat) -> Nat {
    match a {
        Nat::Zero => b.clone(),
        Nat::Succ(p) => Nat::Succ(Box::new(add(p, b))),
    }
}

fn sub(a: &Nat, b: &Nat) -> Nat {
    match (a, b) {
        (_, Nat::Zero) => a.clone(),
        (Nat::Succ(ap), Nat::Succ(bp)) => sub(ap, bp),
        (Nat::Zero, _) => Nat::Zero,
    }
}

fn length<T>(xs: &List<T>) -> Nat {
    match xs {
        List::Nil => Nat::Zero,
        List::Cons(_, t) => Nat::Succ(Box::new(length(t))),
    }
}

fn map<A, B>(f: fn(&A) -> B, xs: &List<A>) -> List<B> {
    match xs {
        List::Nil => List::Nil,
        List::Cons(h, t) => List::Cons(f(h), Box::new(map(f, t))),
    }
}

fn nat_range(n: &Nat) -> List<Nat> {
    match n {
        Nat::Zero => List::Nil,
        _ => {
            let one = from_u64(1);
            List::Cons(n.clone(), Box::new(nat_range(&sub(n, &one))))
        }
    }
}

fn bool_list(n: &Nat) -> List<bool> {
    match n {
        Nat::Zero => List::Nil,
        Nat::Succ(p) => List::Cons(true, Box::new(bool_list(p))),
    }
}

fn nat_to_bool(n: &Nat) -> bool {
    !matches!(n, Nat::Zero)
}

fn bool_to_nat(b: &bool) -> Nat {
    if *b { from_u64(1) } else { Nat::Zero }
}

fn run_once() -> u64 {
    let n50 = from_u64(50);
    let nats = nat_range(&n50);
    let bools = map(nat_to_bool, &nats);
    let nats2 = map(bool_to_nat, &bools);
    let len_nats = length(&nats2);
    let len_bools = length(&bool_list(&n50));
    let result = add(&len_nats, &len_bools);
    to_u64(&result)
}

fn main() {
    let iters: u64 = std::env::args().nth(1).map_or(10000, |s| s.parse().unwrap());
    let mut total: u64 = 0;
    for _ in 0..iters {
        total += run_once();
    }
    println!("{}", total);
}
