//! Phase 2: Type Unfolding
//!
//! Handles μ-type unfolding for recursive ADTs.
//!
//! ## The Two-Phase Substitution Bug (ADR 30.1.26)
//!
//! When unfolding a generic recursive type like `List<String>`:
//!
//! ```text
//! List<String> = μα_List. (Unit + (String × α_List))
//! ```
//!
//! Unfolding naively gives:
//! ```text
//! Unit + (String × α_List)  // ❌ α_List still present!
//! ```
//!
//! We must substitute the μ-variable with the full type:
//! ```text
//! Unit + (String × List<String>)  // ✓ Correct
//! ```
//!
//! This ensures that field types in constructor patterns are correct.

use tungsten_core::{Term, Type};

use crate::elaborate::env::TypeDefKind;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::Elaborator;
use crate::span::Span;

/// Why an inner μ chain could not be resolved to a structural head
/// (ADR 11.8.26c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::elaborate) enum UnflattenedMuCause {
    /// The chain re-entered a binder this call had already unfolded, so no
    /// further iteration can make progress. This is the *nested inductive
    /// family* shape: `type Rose = Node(Wrap<Rose>)` encodes to the vacuous
    /// `μα_Rose. α_Rose`, whose substitution is the identity — the loop that
    /// preceded this guard spun on it forever at flat RSS.
    BinderRepeats,
    /// The environment holds no cached encoding for the binder's type. The
    /// pre-guard code substituted the *current type into its own body* here,
    /// which is the accumulating shape ADR 7.7.26k banned.
    MissingEncoding,
}

/// A μ chain that [`Elaborator::unfold_inner_mu_layers`] refused to keep
/// peeling, carried back to `elab_adt_match` for reporting.
///
/// Reporting happens at the call site rather than here because both unfolding
/// entry points take `&self` while `record_error` takes `&mut self` — and
/// because it is the protocol `unfold_mu_type`'s contract already assumes
/// ("callers treat a residual `Mu` like any other non-structural type and
/// report it at their own dispatch site").
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::elaborate) struct UnflattenedMu {
    /// The μ binder the peel stopped on, e.g. `α_Rose`.
    pub binder: String,
    /// Which of the two non-flattening conditions fired.
    pub cause: UnflattenedMuCause,
}

impl UnflattenedMu {
    /// The source-level type name the binder stands for (`α_Rose` → `Rose`).
    pub fn type_name(&self) -> &str {
        self.binder.strip_prefix("α_").unwrap_or(&self.binder)
    }
}

/// A μ chain whose **outer binder has already been peeled**, ready for
/// [`Elaborator::unfold_inner_mu_layers`].
///
/// # Why this is a type and not a doc comment
///
/// The peel resolves each binder to *its own member's* cached encoding, and
/// that is only the right thing to do on a **residual** chain. Handed a
/// member's whole stored encoding instead, it would resolve the outer binder
/// against the type it is already inside, and a mutual group's members would
/// silently swap — a wrong answer of exactly the kind no test notices, because
/// a wrongly-resolved member is still a well-formed `Type`.
///
/// That precondition governed three call sites while living only in prose, and
/// it is the same confusion that made "is `unfold_mu_type` a drop-in?" hard to
/// answer (ADR 11.8.26c §2.1): the canonical unfolder is sound on the *stored*
/// encoding and unsound here, purely because of which of the two it is given.
///
/// The wrapper does not *prove* the outer binder was peeled — that would need
/// provenance the `Type` does not carry. What it does is make every
/// construction site spell the claim out ([`Self::after_outer_unfold`]), so
/// the coupling is greppable instead of invisible, and a fourth caller has to
/// state it rather than inherit it by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::elaborate) struct ResidualMuChain(Type);

impl ResidualMuChain {
    /// Assert that `ty` is what remains after the outer μ binder was peeled.
    ///
    /// Named for the claim rather than the operation (`new`/`from`) so the
    /// call sites read as the assertion they are making.
    pub(in crate::elaborate) fn after_outer_unfold(ty: Type) -> Self {
        Self(ty)
    }
}

impl<'a> Elaborator<'a> {
    /// Unfold the scrutinee type if it's a recursive μ-type.
    ///
    /// For `μX.F(X)`, unfolding gives `F(μX.F(X))` - we substitute X with the full μ-type.
    /// This is critical for generic ADTs like `List<String> = μα.(Unit + (String × α))`.
    /// See ADR 30.1.26 for details on the two-phase substitution bug.
    ///
    /// Returns `(unfolded_type, unfolded_term)`, or the μ chain that would not
    /// flatten (ADR 11.8.26c) for the caller to report as E0064.
    /// # Cross-Module Type Handling (ADR 30.1.26, ADR 31.1.26)
    ///
    /// When the scrutinee type comes from a cross-module reference, it may be
    /// represented as `Type::App("TypeName", [args])` instead of its structural
    /// encoding. We ALWAYS normalize the type first to expand such references,
    /// not just for recursive types.
    ///
    /// For example, `Option<String>` imported from another module might be:
    /// - `Type::App("Option", [String])` (cross-module, unexpanded)
    /// - `Type::Sum(Unit, String)` (properly encoded)
    ///
    /// Without normalization, the `build_adt_match` function fails with E9999
    /// "expected sum type" because it receives the unexpanded `Type::App`.
    pub(super) fn unfold_scrutinee_type(
        &self,
        scrutinee_ty: &Type,
        scrutinee_term: Term,
        is_recursive: bool,
    ) -> Result<(Type, Term), UnflattenedMu> {
        // ALWAYS normalize to expand cross-module Type::App references to their
        // structural encodings. This is necessary even for non-recursive ADTs
        // because the scrutinee type may come from a cross-module import that
        // stored it as Type::App instead of its sum-type encoding.
        let normalized = self.normalize_for_comparison(scrutinee_ty);

        if is_recursive {
            let mut sum_type = match &normalized {
                Type::Mu(var, body) => {
                    let unfolded = body.substitute(var, &normalized);

                    // --trace-types instrumentation point 5: unfold_scrutinee (ADR 13.4.26c §5)
                    if self.should_trace() {
                        self.trace(
                            "unfold_scrutinee",
                            &format!(
                                "scrutinee_ty: {}\nnormalized: {}\nunfolded: {}",
                                self.format_type_with_provenance(scrutinee_ty),
                                self.format_type_with_provenance(&normalized),
                                unfolded
                            ),
                        );
                    }

                    unfolded
                }
                _ => {
                    if self.should_trace() {
                        self.trace(
                            "unfold_scrutinee",
                            &format!(
                                "WARNING: recursive type normalized to non-Mu: {}",
                                normalized
                            ),
                        );
                    }
                    normalized.clone()
                }
            };

            // Handle nested Mu from mutual recursion (ADR 18.4.26i).
            // Mutually recursive types have nested Mu binders for each group member.
            // After the initial unfold, inner Mu layers remain. We resolve each by
            // substituting the variable with its cached encoding from the environment.
            sum_type =
                self.unfold_inner_mu_layers(ResidualMuChain::after_outer_unfold(sum_type))?;

            let match_scrutinee = Term::unfold(normalized.clone(), scrutinee_term);
            Ok((sum_type, match_scrutinee))
        } else {
            // For non-recursive ADTs, the normalized type should be a Sum
            Ok((normalized, scrutinee_term))
        }
    }

    /// Unfold remaining inner Mu layers from mutually recursive types.
    ///
    /// After a standard Mu unfold, mutually recursive types may still have
    /// nested Mu binders (one per group member). Each inner variable (e.g.,
    /// `α_Expr`) is resolved by substituting it with the cached encoding
    /// of the corresponding type from the environment.
    ///
    /// # Why this is not `tungsten_core::types::unfold_mu_type` (ADR 11.8.26c §2.1)
    ///
    /// The canonical unfolder replaces **every** chain variable with the whole
    /// input μ-type, which is sound for a group encoding read as a whole. This
    /// loop runs on the *residual* chain, after `unfold_scrutinee_type` has
    /// already peeled the outer binder — so the input no longer carries the
    /// group's identity, and each remaining binder must be resolved to *its
    /// own* member's cached encoding instead. `unfold_mu_layers_via_canonical`
    /// in `termination.rs` pins the disagreement on an asymmetric mutual pair.
    ///
    /// # Termination (ADR 11.8.26c §2.1)
    ///
    /// The loop re-enters on a freshly *constructed* type rather than on a
    /// subterm of its predecessor, so — unlike every other μ-peel in
    /// production — it needs an argument beyond "the term shrinks". The
    /// visited-binder list supplies it: `Type::substitute` preserves binder
    /// names rather than alpha-renaming, so binder names come from the finite
    /// set the environment fixes, and refusing to unfold one twice bounds the
    /// iteration count by that set's size. A repeat means no progress —
    /// `μα_Rose. α_Rose` substitutes to itself — so stopping there loses
    /// nothing a further iteration would have found.
    ///
    /// A `Vec` rather than a `HashSet` because a binder chain is one entry per
    /// mutual-recursion group member: linear scan over a handful of strings
    /// beats hashing them.
    pub(in crate::elaborate) fn unfold_inner_mu_layers(
        &self,
        residual: ResidualMuChain,
    ) -> Result<Type, UnflattenedMu> {
        let ResidualMuChain(mut ty) = residual;
        let mut unfolded: Vec<String> = Vec::new();
        while let Type::Mu(ref var, ref body) = ty {
            if unfolded.iter().any(|seen| seen == var) {
                return Err(UnflattenedMu {
                    binder: var.clone(),
                    cause: UnflattenedMuCause::BinderRepeats,
                });
            }
            let type_name = var.strip_prefix("α_").unwrap_or(var);
            let Some(encoding) = self
                .env
                .lookup_type(type_name)
                .and_then(|td| td.encoded_type.clone())
            else {
                return Err(UnflattenedMu {
                    binder: var.clone(),
                    cause: UnflattenedMuCause::MissingEncoding,
                });
            };
            unfolded.push(var.clone());
            ty = body.substitute(var, &encoding);
        }
        Ok(ty)
    }
}

mod diagnosis;

#[cfg(test)]
mod diagnosis_tests;
#[cfg(test)]
mod termination;
#[cfg(test)]
mod test_fixtures;
#[cfg(test)]
mod tests;
