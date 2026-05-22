// Benchmark: String concatenation (structurally equivalent)
//
// Rust baseline uses String concatenation to match Tungsten's approach.
// This is deliberately not using StringBuilder/push_str optimization.
// Prints final length as checksum (44 chars × N reps).
// Default: 15000 repetitions.
// Usage: ./string_concat [repetitions]  (default: 15000)

fn repeat(s: &str, n: u64) -> String {
    if n == 0 {
        String::new()
    } else {
        format!("{}{}", s, repeat(s, n - 1))
    }
}

fn main() {
    let n: u64 = std::env::args().nth(1).map_or(15000, |s| s.parse().unwrap());
    let result = repeat("the quick brown fox jumps over the lazy dog!", n);
    println!("{}", result.len());
}
