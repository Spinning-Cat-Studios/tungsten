//! Sorry-site classification (ADR 18.9.26g).
//!
//! `contains_sorry` answers "is there a hole?"; this walk answers "whose hole
//! is it?". The elaborator wraps every hole an author writes — `sorry` and the
//! body of an `axiom` — in `Term::Spanned`, and the pattern lowering plants its
//! own holes bare, under binders no author needs to write. So the term alone
//! says who wrote each hole, with no parsed item in reach.

use crate::terms::{Term, TermSpan};

/// The lowering construct that planted a bare hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Construct {
    /// An `AdtMatch` arm the pattern cannot reach, bound as `__unreachable_<n>`.
    UnreachablePatternArm,
    /// A binary `Case` branch the pattern cannot reach, bound as `__abs<n>`.
    AbsurdBranch,
}

impl Construct {
    /// The name a diagnostic prints for this construct.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Construct::UnreachablePatternArm => "unreachable pattern arm",
            Construct::AbsurdBranch => "absurd branch",
        }
    }
}

/// Who put one `Sorry` leaf into the term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SorrySite {
    /// Written by the author: a `Sorry` directly under `Spanned`.
    Authored(TermSpan),
    /// Planted by a known lowering construct.
    Synthesised(Construct),
    /// Any other bare `Sorry` — not written by the author, producer unnamed.
    Unclassified,
}

/// Per-class totals over one or more terms.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SorryCounts {
    pub authored: usize,
    pub synthesised: usize,
    pub unclassified: usize,
}

impl SorryCounts {
    /// Sum the sites of every term.
    pub fn of<'a>(terms: impl IntoIterator<Item = &'a Term>) -> Self {
        let mut counts = SorryCounts::default();
        for term in terms {
            counts.add_sites(&term.sorry_sites());
        }
        counts
    }

    /// Add one term's sites to the totals.
    pub fn add_sites(&mut self, sites: &[SorrySite]) {
        for site in sites {
            match site {
                SorrySite::Authored(_) => self.authored += 1,
                SorrySite::Synthesised(_) => self.synthesised += 1,
                SorrySite::Unclassified => self.unclassified += 1,
            }
        }
    }

    /// Holes the author did not write: synthesised plus unclassified.
    #[must_use]
    pub fn not_authored(&self) -> usize {
        self.synthesised + self.unclassified
    }

    /// Every hole, whoever wrote it.
    #[must_use]
    pub fn total(&self) -> usize {
        self.authored + self.not_authored()
    }
}

/// `true` when `name` is `prefix` followed by one or more ASCII digits.
fn is_marker_binder(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// The construct a bare hole names when it is the entire body under `binder`.
fn construct_for_arm_binder(binder: &str) -> Option<Construct> {
    is_marker_binder(binder, "__unreachable_").then_some(Construct::UnreachablePatternArm)
}

fn construct_for_case_binder(binder: &str) -> Option<Construct> {
    is_marker_binder(binder, "__abs").then_some(Construct::AbsurdBranch)
}

impl Term {
    /// One entry per `Sorry` leaf, in pre-order.
    #[must_use]
    pub fn sorry_sites(&self) -> Vec<SorrySite> {
        let mut sites = Vec::new();
        collect_sites(self, None, &mut sites);
        sites
    }
}

/// `planted_by` is set only when `term` is the entire body under a marker
/// binder; any intervening node resets it, so a hole nested deeper is
/// `Unclassified`.
fn collect_sites(term: &Term, planted_by: Option<Construct>, sites: &mut Vec<SorrySite>) {
    match term {
        Term::Sorry => {
            sites.push(planted_by.map_or(SorrySite::Unclassified, SorrySite::Synthesised));
        }
        Term::Spanned(inner, span) if matches!(**inner, Term::Sorry) => {
            sites.push(SorrySite::Authored(*span));
        }
        Term::AdtMatch(scrutinee, arms) => {
            collect_sites(scrutinee, None, sites);
            for (_, binder, body) in arms {
                collect_sites(body, construct_for_arm_binder(binder), sites);
            }
        }
        Term::Case(scrutinee, left_binder, left, right_binder, right) => {
            collect_sites(scrutinee, None, sites);
            collect_sites(left, construct_for_case_binder(left_binder), sites);
            collect_sites(right, construct_for_case_binder(right_binder), sites);
        }
        _ => term.for_each_subterm(|child| collect_sites(child, None, sites)),
    }
}
