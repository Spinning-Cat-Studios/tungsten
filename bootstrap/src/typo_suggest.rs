//! Top-level subcommand typo detection.
//!
//! When the user types `tungsten doctro`, clap's positional `file` argument
//! captures it before subcommand matching can fire. This module provides
//! Levenshtein-based "did you mean?" suggestions so the user gets a helpful
//! hint instead of a confusing "file not found" error.

/// Top-level subcommand names for typo detection.
const TOP_LEVEL_SUBCOMMANDS: &[&str] = &[
    "check", "run", "test", "compile", "eval", "repl", "clean", "cache", "info", "explain",
    "doctor", "diff", "sidecar", "commands",
];

/// If `input` is within edit-distance 2 of a known subcommand, return the best match.
pub fn suggest_subcommand(input: &str) -> Option<&'static str> {
    let mut best: Option<(&str, usize)> = None;
    for &cmd in TOP_LEVEL_SUBCOMMANDS {
        let d = levenshtein(input, cmd);
        if d <= 2 && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((cmd, d));
        }
    }
    best.map(|(cmd, _)| cmd)
}

/// Levenshtein edit distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein("doctor", "doctor"), 0);
    }

    #[test]
    fn levenshtein_one_char_swap() {
        // transposition = 2 ops in plain Levenshtein (delete + insert)
        assert_eq!(levenshtein("doctro", "doctor"), 2);
    }

    #[test]
    fn levenshtein_one_char_missing() {
        assert_eq!(levenshtein("docto", "doctor"), 1);
    }

    #[test]
    fn levenshtein_completely_different() {
        assert!(levenshtein("xyz", "doctor") > 2);
    }

    #[test]
    fn suggest_finds_doctor_from_doctro() {
        assert_eq!(suggest_subcommand("doctro"), Some("doctor"));
    }

    #[test]
    fn suggest_finds_info_from_inf() {
        assert_eq!(suggest_subcommand("inf"), Some("info"));
    }

    #[test]
    fn suggest_finds_explain_from_explan() {
        assert_eq!(suggest_subcommand("explan"), Some("explain"));
    }

    #[test]
    fn suggest_finds_diff_from_dif() {
        assert_eq!(suggest_subcommand("dif"), Some("diff"));
    }

    #[test]
    fn suggest_returns_none_for_unrelated() {
        assert_eq!(suggest_subcommand("foobar"), None);
    }

    #[test]
    fn suggest_returns_none_for_tg_file() {
        // Actual .tg files should not trigger subcommand suggestions
        assert_eq!(suggest_subcommand("hello.tg"), None);
    }

    #[test]
    fn suggest_exact_match() {
        assert_eq!(suggest_subcommand("doctor"), Some("doctor"));
    }

    #[test]
    fn suggest_finds_commands_from_comands() {
        assert_eq!(suggest_subcommand("comands"), Some("commands"));
    }

    #[test]
    fn suggest_finds_sidecar_from_sidcar() {
        assert_eq!(suggest_subcommand("sidcar"), Some("sidecar"));
    }
}
