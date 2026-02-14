//! Utility functions for the Tungsten compiler.
//!
//! General-purpose utilities including string algorithms.

use crate::config::{SUGGESTION_MAX_DISTANCE, SUGGESTION_MAX_RATIO};

/// Compute the Levenshtein edit distance between two strings.
///
/// The edit distance is the minimum number of single-character edits
/// (insertions, deletions, or substitutions) required to change one
/// string into the other.
///
/// # Examples
///
/// ```
/// use tungsten_bootstrap::utils::levenshtein_distance;
///
/// assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
/// assert_eq!(levenshtein_distance("Nat", "Nta"), 2);
/// assert_eq!(levenshtein_distance("", "abc"), 3);
/// ```
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    // Early termination for empty strings
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    // Use two rows instead of full matrix for space efficiency
    let mut prev_row: Vec<usize> = (0..=n).collect();
    let mut curr_row: Vec<usize> = vec![0; n + 1];

    for i in 1..=m {
        curr_row[0] = i;

        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };

            curr_row[j] = (prev_row[j] + 1) // deletion
                .min(curr_row[j - 1] + 1) // insertion
                .min(prev_row[j - 1] + cost); // substitution
        }

        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[n]
}

/// Check if a name is similar enough to suggest as a typo correction.
///
/// Uses the thresholds defined in [`crate::config`]:
/// - Edit distance must be ≤ `SUGGESTION_MAX_DISTANCE`
/// - Edit distance ratio to target length must be ≤ `SUGGESTION_MAX_RATIO`
pub fn is_similar_name(typo: &str, candidate: &str) -> bool {
    let distance = levenshtein_distance(typo, candidate);

    if distance > SUGGESTION_MAX_DISTANCE {
        return false;
    }

    // Ratio check to avoid suggesting very different names for short identifiers
    let target_len = candidate.len().max(1);
    let ratio = distance as f64 / target_len as f64;

    ratio <= SUGGESTION_MAX_RATIO
}

/// Find the best suggestion from a list of candidates for a typo.
///
/// Returns `None` if no candidate is similar enough according to [`is_similar_name`].
/// If multiple candidates have the same distance, returns the first one found.
pub fn find_best_suggestion<'a>(
    typo: &str,
    candidates: impl Iterator<Item = &'a str>,
) -> Option<&'a str> {
    let mut best: Option<(&str, usize)> = None;

    for candidate in candidates {
        if !is_similar_name(typo, candidate) {
            continue;
        }

        let distance = levenshtein_distance(typo, candidate);

        match best {
            None => best = Some((candidate, distance)),
            Some((_, best_dist)) if distance < best_dist => {
                best = Some((candidate, distance));
            }
            _ => {}
        }
    }

    best.map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_empty_strings() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("", "abc"), 3);
    }

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
        assert_eq!(levenshtein_distance("Nat", "Nat"), 0);
    }

    #[test]
    fn test_levenshtein_simple() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("Nat", "Nta"), 2);
        assert_eq!(levenshtein_distance("Bool", "bool"), 1);
        assert_eq!(levenshtein_distance("String", "Strng"), 1);
    }

    #[test]
    fn test_levenshtein_single_edit() {
        assert_eq!(levenshtein_distance("cat", "car"), 1); // substitution
        assert_eq!(levenshtein_distance("cat", "cats"), 1); // insertion
        assert_eq!(levenshtein_distance("cats", "cat"), 1); // deletion
    }

    #[test]
    fn test_is_similar_name() {
        // Similar names (within threshold)
        assert!(is_similar_name("bool", "Bool")); // distance 1, ratio 0.25
        assert!(is_similar_name("Strng", "String")); // distance 1, ratio 0.17
        assert!(is_similar_name("Optoin", "Option")); // distance 2, ratio 0.33

        // Borderline - "Nta" to "Nat" is distance 2, ratio 0.67 > 0.5
        assert!(!is_similar_name("Nta", "Nat")); // ratio too high

        // Too different
        assert!(!is_similar_name("Result", "Option"));

        // Short name with high ratio - "x" to "foo" is distance 3, ratio 1.0
        assert!(!is_similar_name("x", "foo"));
    }

    #[test]
    fn test_find_best_suggestion() {
        let candidates = vec!["Nat", "Bool", "String", "Option", "Result"];

        // "bool" to "Bool" is distance 1, ratio 0.25 - should match
        assert_eq!(
            find_best_suggestion("bool", candidates.iter().map(|s| *s)),
            Some("Bool")
        );

        // "Strng" to "String" is distance 1, ratio 0.17 - should match
        assert_eq!(
            find_best_suggestion("Strng", candidates.iter().map(|s| *s)),
            Some("String")
        );

        // No good suggestion
        assert_eq!(
            find_best_suggestion("xyz", candidates.iter().map(|s| *s)),
            None
        );
    }
}
