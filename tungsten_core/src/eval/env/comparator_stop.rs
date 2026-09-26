//! Why a structural comparison never ran (ADR 1.8.26b D3).
//!
//! `compare<T>` resolves through the env's [`ComparatorSynth`] callback. Before
//! this ADR the callback's only failure signal was `None`, which the evaluator
//! turned into `StepResult::Stuck` — and a stuck term is a *normal* outcome, so
//! the enclosing `assert_eq` never executed, the failure flag was never set, and
//! `tungsten test` reported the test **`ok`**. A suite of only-passing
//! assertions cannot tell that apart from success.
//!
//! The fix mirrors the black-hole treatment (ADR 22.7.26a): interior stepping
//! still yields `Stuck` (the loops must terminate), but the *reason* rides the
//! env and every entry point surfaces it as a distinct [`EvalStopped`] rather
//! than as a value.
//!
//! [`ComparatorSynth`]: super::ComparatorSynth

use std::fmt;

/// A `compare<T>` that could not be turned into a runnable comparator.
///
/// `type_name` is the rendered `T` — the callback owns the `Type`, and the
/// evaluator only ever reports it, so a `String` keeps `tungsten_core` free of
/// any dependency on how `bootstrap` spells a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparatorFailure {
    /// The concrete type the comparison was requested at.
    pub type_name: String,
    /// Which of the enumerated outcomes this is.
    pub kind: ComparatorFailureKind,
}

impl ComparatorFailure {
    /// Build a failure at `type_name`.
    #[must_use]
    pub fn new(type_name: impl Into<String>, kind: ComparatorFailureKind) -> Self {
        ComparatorFailure {
            type_name: type_name.into(),
            kind,
        }
    }
}

/// The enumerated ways synthesis can fail (ADR 1.8.26b §3.1).
///
/// Enumerated rather than collapsed to one "not comparable" because the fixes
/// differ, and because the ADR's own close-out gate — is a residual `Expr` wall
/// a *cost* wall or a *correctness* wall? — is decided by **which** of these the
/// gate reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparatorFailureKind {
    /// A §2.2-noncomparable leaf (a closure, an arena handle) makes the
    /// enclosing type noncomparable *by policy*. `path` locates the first one.
    OpaqueLeaf { path: String },
    /// Nothing at all was synthesizable for `T`.
    EmptyClosure,
    /// A synthesized body calls a `compare_*` symbol the closure never defines,
    /// which a `closure.is_empty()` predicate cannot express because the
    /// closure is *non-empty* and internally self-consistent.
    ///
    /// `cause` locates why the sub-type under that symbol could not be
    /// synthesized, when the walk knows. Reporting the dangling symbol alone
    /// was measured to be actively misleading: `compare_AdtExpr_E` reads like a
    /// recursion problem and is in fact an unsupported field type several
    /// levels down.
    IncompleteClosure {
        dangling: String,
        cause: Option<String>,
    },
    /// Synthesis emitted more comparators than the bound allows without
    /// settling. Distinguishes "too expensive" from "wrong", which is exactly
    /// the classification the ADR's `Expr` criterion turns on.
    LimitExceeded { bound: usize },
    /// No synthesis callback is installed on this environment at all.
    NoSynthesizer,
}

/// Append the "because …" clause when the walk attributed one.
///
/// A dangling symbol names the shape that could not be built; the cause names
/// why, and the two are usually in different parts of the type. Absent when the
/// walk could not attribute it — the message must not invent a reason.
fn write_cause(f: &mut fmt::Formatter<'_>, cause: Option<&str>) -> fmt::Result {
    match cause {
        Some(cause) => write!(f, " because {cause}"),
        None => Ok(()),
    }
}

impl fmt::Display for ComparatorFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot compare `{}`: ", self.type_name)?;
        match &self.kind {
            ComparatorFailureKind::OpaqueLeaf { path } => {
                write!(f, "{path} is not comparable")
            }
            ComparatorFailureKind::EmptyClosure => {
                write!(f, "no comparator could be synthesized for this type")
            }
            ComparatorFailureKind::IncompleteClosure { dangling, cause } => {
                write!(
                    f,
                    "the synthesized comparator calls `{dangling}`, which was never defined"
                )?;
                write_cause(f, cause.as_deref())
            }
            ComparatorFailureKind::LimitExceeded { bound } => write!(
                f,
                "comparator synthesis did not settle within {bound} definitions"
            ),
            ComparatorFailureKind::NoSynthesizer => {
                write!(f, "no comparator synthesizer is installed")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_the_type_and_the_dangling_symbol() {
        let f = ComparatorFailure::new(
            "Alpha",
            ComparatorFailureKind::IncompleteClosure {
                dangling: "compare_Mu_X".to_string(),
                cause: None,
            },
        );
        let rendered = f.to_string();
        assert!(rendered.contains("Alpha"), "{rendered}");
        assert!(rendered.contains("compare_Mu_X"), "{rendered}");
    }

    #[test]
    fn display_distinguishes_every_kind() {
        let kinds = [
            ComparatorFailureKind::OpaqueLeaf {
                path: "$.env".to_string(),
            },
            ComparatorFailureKind::EmptyClosure,
            ComparatorFailureKind::IncompleteClosure {
                dangling: "compare_X".to_string(),
                cause: None,
            },
            ComparatorFailureKind::LimitExceeded { bound: 512 },
            ComparatorFailureKind::NoSynthesizer,
        ];
        let rendered: Vec<String> = kinds
            .iter()
            .map(|k| ComparatorFailure::new("T", k.clone()).to_string())
            .collect();
        for (i, a) in rendered.iter().enumerate() {
            for b in rendered.iter().skip(i + 1) {
                assert_ne!(a, b, "two kinds render identically");
            }
        }
    }

    #[test]
    fn an_incomplete_closure_names_its_cause_when_the_walk_knows_one() {
        let f = ComparatorFailure::new(
            "Expr",
            ComparatorFailureKind::IncompleteClosure {
                dangling: "compare_AdtExpr_E".to_string(),
                cause: Some("$.type_params: List<TypeParam> is opaque".to_string()),
            },
        );
        let rendered = f.to_string();
        assert!(rendered.contains("compare_AdtExpr_E"), "{rendered}");
        assert!(
            rendered.contains("$.type_params"),
            "the symbol alone is misleading; the cause must appear: {rendered}"
        );
    }

    #[test]
    fn limit_exceeded_names_the_bound() {
        let f = ComparatorFailure::new("Expr", ComparatorFailureKind::LimitExceeded { bound: 512 });
        assert!(f.to_string().contains("512"), "{f}");
    }

    #[test]
    fn opaque_leaf_names_the_path() {
        let f = ComparatorFailure::new(
            "Env",
            ComparatorFailureKind::OpaqueLeaf {
                path: "$.closure".to_string(),
            },
        );
        assert!(f.to_string().contains("$.closure"), "{f}");
    }
}
