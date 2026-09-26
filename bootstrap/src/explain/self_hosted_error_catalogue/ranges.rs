//! The self-hosted compiler's error-code **ranges**, as data (ADR 18.8.26b D4).
//!
//! Its scheme is range-based where the bootstrap's is flat, so a new gate does
//! not pick a next free number — it claims a block. That makes the ranges the
//! thing worth asserting: a block that overlapped another would silently give
//! two unrelated failures the same code, and nothing downstream would notice,
//! because every consumer looks a code up rather than classifying it.
//!
//! E0700–E0799 is the soundness block, subdivided by gate: **positivity holds
//! E0700–E0709 and termination E0710–E0719** (ADR 19.8.26d D3), leaving
//! E0720–E0799 reserved for later soundness gates. The subdivision is a
//! convention rather than a checked range — what *is* checked is that no code
//! outside a catalogued entry resolves, so a later gate finds its number free.

/// One contiguous block of the self-hosted numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CodeRange {
    pub(super) start: u32,
    pub(super) end: u32,
    pub(super) name: &'static str,
}

impl CodeRange {
    pub(super) const fn contains(&self, code: u32) -> bool {
        self.start <= code && code <= self.end
    }

    /// Whether two blocks share any code at all.
    pub(super) const fn overlaps(&self, other: &CodeRange) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

/// Every declared block, mirroring the header comment of
/// `src/compiler/elab/error/kinds.tg`.
pub(super) const RANGES: &[CodeRange] = &[
    CodeRange {
        start: 1,
        end: 99,
        name: "Type errors",
    },
    CodeRange {
        start: 100,
        end: 199,
        name: "Name resolution",
    },
    CodeRange {
        start: 200,
        end: 299,
        name: "Items",
    },
    CodeRange {
        start: 300,
        end: 399,
        name: "Patterns",
    },
    CodeRange {
        start: 400,
        end: 499,
        name: "Proofs",
    },
    CodeRange {
        start: 500,
        end: 599,
        name: "References",
    },
    CodeRange {
        start: 600,
        end: 699,
        name: "Entry point / control flow",
    },
    CodeRange {
        start: 700,
        end: 799,
        name: "Soundness gates",
    },
    CodeRange {
        start: 900,
        end: 999,
        name: "Other / internal",
    },
];

/// The soundness block, and the half of it positivity claims.
pub(super) const SOUNDNESS: CodeRange = CodeRange {
    start: 700,
    end: 799,
    name: "Soundness gates",
};
pub(super) const POSITIVITY: CodeRange = CodeRange {
    start: 700,
    end: 709,
    name: "Strict positivity",
};

/// Parse an `Ennnn` code to its number. `None` on anything else.
pub(super) fn code_number(code: &str) -> Option<u32> {
    code.strip_prefix('E')?.parse().ok()
}

/// The block `code` falls in, if any.
pub(super) fn range_of(code: u32) -> Option<&'static CodeRange> {
    RANGES.iter().find(|range| range.contains(code))
}

/// The catalogue the range table describes. Imported at module level rather
/// than inside `tests` so the path stays one `super` deep.
use super::entries::SELF_HOSTED_ERRORS;

#[cfg(test)]
mod tests {
    use super::*;

    /// AC7. The **whole block** must be disjoint from every other range, not
    /// merely resolvable at one code: a partial overlap would leave the
    /// successor's E0710 colliding with something already spoken for, and the
    /// collision would only surface when that successor was written.
    #[test]
    fn the_soundness_block_is_disjoint_from_every_other_range() {
        for range in RANGES {
            if range == &SOUNDNESS {
                continue;
            }
            assert!(
                !SOUNDNESS.overlaps(range),
                "E0700-E0799 overlaps `{}` (E{:04}-E{:04})",
                range.name,
                range.start,
                range.end
            );
        }
    }

    /// The same property for every pair, so a future block cannot be added
    /// overlapping one that is not the soundness one.
    #[test]
    fn no_two_ranges_overlap() {
        for (index, left) in RANGES.iter().enumerate() {
            for right in &RANGES[index + 1..] {
                assert!(
                    !left.overlaps(right),
                    "`{}` overlaps `{}`",
                    left.name,
                    right.name
                );
            }
        }
    }

    /// Positivity's half is inside the soundness block and leaves E0710 free
    /// for the termination mirror (D4).
    #[test]
    fn positivity_claims_the_first_ten_and_leaves_e0710_free() {
        assert!(SOUNDNESS.contains(POSITIVITY.start) && SOUNDNESS.contains(POSITIVITY.end));
        assert_eq!(
            POSITIVITY.end + 1,
            710,
            "the termination mirror starts here"
        );
        assert!(SOUNDNESS.contains(710));
    }

    /// Every catalogued code lands in exactly one block. This is what makes
    /// the ranges a *description* of the catalogue rather than a parallel
    /// table that can drift from it.
    #[test]
    fn every_catalogued_code_falls_in_exactly_one_range() {
        for entry in SELF_HOSTED_ERRORS {
            let number = code_number(entry.code)
                .unwrap_or_else(|| panic!("`{}` is not an Ennnn code", entry.code));
            let matches = RANGES.iter().filter(|range| range.contains(number)).count();
            assert_eq!(matches, 1, "`{}` falls in {matches} range(s)", entry.code);
        }
    }

    /// The mirror's own code is catalogued, in the block it claims — the
    /// failure this would have caught is a code that elaborates and explains
    /// as "unknown".
    #[test]
    fn the_positivity_code_is_catalogued_in_its_own_range() {
        let entry = SELF_HOSTED_ERRORS
            .iter()
            .find(|entry| entry.code == "E0700")
            .expect("E0700 is catalogued");
        assert_eq!(entry.name, "ErrNonStrictlyPositive");
        assert!(POSITIVITY.contains(code_number(entry.code).expect("Ennnn")));
        assert_eq!(
            range_of(700).map(|range| range.name),
            Some("Soundness gates")
        );
    }

    /// A code outside every block resolves to nothing rather than to the
    /// nearest one — the gap at E0800-E0899 is deliberate.
    #[test]
    fn a_code_in_no_range_resolves_to_none() {
        assert_eq!(range_of(850), None);
        assert_eq!(code_number("nonsense"), None);
        assert_eq!(code_number("E0700"), Some(700));
    }
}
