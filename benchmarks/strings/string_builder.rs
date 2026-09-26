// Benchmark: String building through a growable buffer (structurally equivalent)
//
// Rust baseline uses String::push_str — one buffer, amortised doubling — to
// match Tungsten's StringBuilder (ADR 14.9.26a). Same workload as
// string_concat, so the two rows compare the append strategies, not the input.
// Prints final length as checksum (44 chars × N reps).
// Default: 15000 repetitions.
// Usage: ./string_builder [repetitions]  (default: 15000)

fn push_repeated(mut buf: String, s: &str, n: u64) -> String {
    if n == 0 {
        buf
    } else {
        buf.push_str(s);
        push_repeated(buf, s, n - 1)
    }
}

fn main() {
    let n: u64 = std::env::args().nth(1).map_or(15000, |s| s.parse().unwrap());
    let result = push_repeated(
        String::new(),
        "the quick brown fox jumps over the lazy dog!",
        n,
    );
    println!("{}", result.len());
}
