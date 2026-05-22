// Benchmark: Closure chains (structurally equivalent)
//
// Rust baseline uses Box<dyn Fn> closures to match Tungsten's heap-allocated
// closure representation.
//
// compose(add_n(3), add_n(2)) = add 5, applied 10 times to 0 = 50.
// Map add_5 over range(20), sum = 310. Total = 360.
// 5000 iterations for ~200ms runtime.

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

fn add(a: &Nat, b: &Nat) -> Nat {
    match a {
        Nat::Zero => b.clone(),
        Nat::Succ(p) => Nat::Succ(Box::new(add(p, b))),
    }
}

fn compose(
    f: Box<dyn Fn(&Nat) -> Nat>,
    g: Box<dyn Fn(&Nat) -> Nat>,
) -> Box<dyn Fn(&Nat) -> Nat> {
    Box::new(move |x| f(&g(x)))
}

fn add_n(n: Nat) -> Box<dyn Fn(&Nat) -> Nat> {
    Box::new(move |x: &Nat| add(x, &n))
}

fn apply_n_times(f: &dyn Fn(&Nat) -> Nat, n: &Nat, x: Nat) -> Nat {
    match n {
        Nat::Zero => x,
        Nat::Succ(p) => apply_n_times(f, p, f(&x)),
    }
}

fn map(f: &dyn Fn(&Nat) -> Nat, xs: &List) -> List {
    match xs {
        List::Nil => List::Nil,
        List::Cons(h, t) => List::Cons(f(h), Box::new(map(f, t))),
    }
}

fn sum(xs: &List) -> Nat {
    match xs {
        List::Nil => Nat::Zero,
        List::Cons(h, t) => add(h, &sum(t)),
    }
}

fn range(n: &Nat) -> List {
    match n {
        Nat::Zero => List::Nil,
        _ => {
            let one = from_u64(1);
            let pred = match n { Nat::Succ(p) => p.as_ref().clone(), _ => Nat::Zero };
            List::Cons(n.clone(), Box::new(range(&pred)))
        }
    }
}

fn run_once() -> u64 {
    let f = compose(add_n(from_u64(3)), add_n(from_u64(2)));
    let ten = from_u64(10);
    let result1 = apply_n_times(&*f, &ten, from_u64(0));

    let twenty = from_u64(20);
    let xs = range(&twenty);
    let mapped = map(&*f, &xs);
    let sum_mapped = sum(&mapped);

    let result = add(&result1, &sum_mapped);
    to_u64(&result)
}

fn main() {
    let iters: u64 = std::env::args().nth(1).map_or(5000, |s| s.parse().unwrap());
    let mut total: u64 = 0;
    for _ in 0..iters {
        total += run_once();
    }
    println!("{}", total);
}
