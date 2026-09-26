//! `Mode::Run` evaluation: find `main`, evaluate it on the environment
//! evaluator, and report the outcome.
//!
//! A black hole (ADR 22.7.26a — a global re-entering its own forcing) is a
//! diagnostic plus a failing exit here, where pre-detection it was a
//! stack-overflow abort. Split from `pipeline.rs` to keep that file under the
//! size limit.

use std::collections::{HashMap, HashSet};

use tungsten_core::{
    eval::{eval_with_env, EvalStopped},
    Term,
};

use crate::elaborate::ElabOutput;

use crate::driver::{render_diagnostics, PipelineResult};

/// Evaluate `main` for `tungsten run`, or report the no-main error (E0030).
pub(super) fn evaluate_main(output: &ElabOutput, source: &str, filename: &str) -> PipelineResult {
    let Some(main_def) = output.defs.iter().find(|d| d.name == "main") else {
        // Point to end of file since we don't have a better location.
        let eof_span = crate::span::Span::new(source.len() as u32, source.len() as u32);
        let err = crate::ElabError::no_main_function(eof_span);
        render_diagnostics(source, filename, &[err], &[]);
        return PipelineResult::Failed;
    };

    // Build globals map (excluding main) for environment-based evaluation.
    // This avoids exponential term blowup from naive substitution. The env
    // also carries the lazy `__cmp<T>` synthesis callback (ADR 29.6.26f
    // §T11.2a / P6′) so the generic `compare` intrinsic resolves.
    let globals: HashMap<String, Term> = output
        .defs
        .iter()
        .filter(|d| d.name != "main")
        .map(|d| (d.name.clone(), d.term.term.clone()))
        .collect();
    let static_defs: HashSet<String> = globals.keys().cloned().collect();
    let types = crate::comparator::ComparatorTypes::new(
        output.record_types.clone(),
        &output.encoded_types,
        &output.type_provenance,
        output.adt_types.clone(),
        &output.mutual_recursion_groups,
    );
    let env = crate::comparator::eval::eval_env(globals, &types);
    match eval_with_env(&main_def.term.term, &env) {
        Ok(value) => {
            // The small-step loop stops when no rule applies and hands back the
            // residual term. Printed bare that is a wall of Core IR in which the
            // actual cause is one token among thousands, so name it first.
            // Comparators are synthesized lazily into the env, so the static
            // def list alone would report every one of them "never resolved"
            // (ADR 1.8.26b).
            let mut defined = static_defs;
            defined.extend(env.registered_comparators());
            if let Some(diagnosis) = stuck_diagnosis(&value, &defined) {
                eprintln!("warning: evaluation got stuck — the result below is a residual term, not a value");
                eprintln!("  = stuck at: {diagnosis}");
            }
            PipelineResult::Evaluated {
                value,
                ty: main_def.ty.clone(),
            }
        }
        // The program has no value: a black hole (ADR 22.7.26a) or a
        // comparison that never ran (ADR 1.8.26b D3). Both used to surface as
        // an ordinary residual, which `run` printed with a warning and a
        // success exit.
        Err(stopped) => {
            eprintln!("error: {stopped}");
            if let Some(note) = stopped_note(&stopped) {
                eprintln!("  = note: {note}");
            }
            PipelineResult::Failed
        }
    }
}

/// The follow-up line under a stopped evaluation — the reader's next move,
/// which differs per stop. `None` where the `Display` already says everything.
pub(super) fn stopped_note(stopped: &EvalStopped) -> Option<&'static str> {
    match stopped {
        EvalStopped::BlackHole { .. } => Some("the definition re-enters itself while being forced"),
        EvalStopped::Uncomparable(_) => Some(
            "run `tungsten doctor check comparable <T> <file>` (cost 3) to see \
             where the comparator breaks",
        ),
        EvalStopped::ComparisonNeverRan { .. } => Some(
            "the assertion consumed a comparison that never produced a result, \
             so it asserted nothing (ADR 1.8.26b)",
        ),
        EvalStopped::MalformedElimination { .. } => Some("see docs/repo-memory/adt-abi-safety.md"),
        // The `Display` line IS the native program's stderr line (ADR
        // 14.9.26c AC 3): nothing may follow it, or the two sides diverge.
        EvalStopped::IntTrap { .. } => None,
        EvalStopped::TimedOut { .. } | EvalStopped::StepLimit { .. } => None,
    }
}

/// One-line explanation of why an evaluated term is not a value, or `None`
/// when it is one.
///
/// A **projection applied to a non-pair** is checked first, because it is
/// unconditional: `Fst` of a scalar can never step, whatever else is in scope.
/// It is what a constructor-payload nesting mismatch looks like at runtime
/// (`docs/repo-memory/adt-abi-safety.md`). Since ADR 1.8.26b D3 that shape
/// never reaches here on the `run` path — the evaluator records it as
/// `EvalStopped::MalformedElimination` and the run fails before a residual is
/// printed — so this arm is a **backstop** for any future entry point that
/// consults a residual without that recording, and the unit tests below are
/// what keep it honest.
///
/// A **`Global` absent from `defined`** is checked second, and only then,
/// because its absence is weaker evidence: a global sitting in a branch the
/// evaluator never reached was not "unresolved" — it was never forced. Getting
/// this order wrong blames `compare_Nat` for a residual whose actual cause is a
/// `Fst(Succ(Zero))` three nodes away, which is what the first cut did.
///
/// `defined` must include the env's **synthesized** comparators, not just the
/// static def list. Comparators are registered lazily during evaluation, so a
/// static-only set reports every one of them "never resolved" — a false lead
/// that cost ADR 1.8.26b its first D2 root-cause thesis. `evaluate_main` unions
/// them in; a caller that forgets will get confidently wrong answers.
///
/// Anything else falls back to naming the head constructor, which is still
/// more than the caller had before.
pub(super) fn stuck_diagnosis(term: &Term, defined: &HashSet<String>) -> Option<String> {
    if term.is_value() {
        return None;
    }
    if let Some(projection) = first_stuck_projection(term) {
        return Some(format!(
            "`{projection}` applied to a non-pair — usually a constructor-payload nesting \
             mismatch (see docs/repo-memory/adt-abi-safety.md)"
        ));
    }
    if let Some(name) = first_undefined_global(term, defined) {
        if name.starts_with("compare_") || name == "__cmp" {
            return Some(format!(
                "comparator `{name}` never resolved — no comparator was synthesized for \
                 this type, so `compare` never ran (try `tungsten doctor check comparable <T> <file>`)"
            ));
        }
        return Some(format!("global `{name}` never resolved"));
    }
    Some(format!("{} (no evaluation rule applies)", head_label(term)))
}

/// Depth-first search for a `Global` the environment does not define.
fn first_undefined_global(term: &Term, defined: &HashSet<String>) -> Option<String> {
    if let Term::Global(name) = term {
        if !defined.contains(name) {
            return Some(name.clone());
        }
    }
    child_terms(term)
        .into_iter()
        .find_map(|child| first_undefined_global(child, defined))
}

/// Depth-first search for `Fst`/`Snd` applied to a value that is not a pair.
fn first_stuck_projection(term: &Term) -> Option<&'static str> {
    let here = match term {
        Term::Fst(inner) if inner.is_value() && !is_pair(inner) => Some("Fst"),
        Term::Snd(inner) if inner.is_value() && !is_pair(inner) => Some("Snd"),
        _ => None,
    };
    here.or_else(|| {
        child_terms(term)
            .into_iter()
            .find_map(first_stuck_projection)
    })
}

fn is_pair(term: &Term) -> bool {
    match term {
        Term::Pair(_, _) => true,
        Term::Spanned(inner, _) => is_pair(inner),
        _ => false,
    }
}

/// The term's constructor name, for the fallback message.
fn head_label(term: &Term) -> &'static str {
    match term {
        Term::App(_, _) => "application",
        Term::Case(_, _, _, _, _) => "case",
        Term::AdtMatch(_, _) => "match",
        Term::Fst(_) | Term::Snd(_) => "projection",
        Term::Unfold(_, _) => "unfold",
        Term::ExternCall(_, _) => "extern call",
        Term::Global(_) => "global",
        _ => "term",
    }
}

/// Immediate subterms, for the generic walks above.
///
/// Deliberately structural rather than exhaustive-by-variant: the two searches
/// only need to *reach* every subterm, and a `_ => vec![]` arm on a large enum
/// keeps this from breaking every time a variant is added.
fn child_terms(term: &Term) -> Vec<&Term> {
    match term {
        Term::App(f, a) => vec![f, a],
        Term::Pair(a, b) => vec![a, b],
        Term::Fst(t) | Term::Snd(t) => vec![t],
        Term::Inl(_, t) | Term::Inr(_, t) => vec![t],
        Term::Fold(_, t) | Term::Unfold(_, t) => vec![t],
        Term::Succ(t) => vec![t],
        Term::Spanned(t, _) => vec![t],
        Term::Annot(t, _) => vec![t],
        Term::TyApp(t, _) => vec![t],
        Term::Lambda(_, _, body) | Term::TyAbs(_, body) => vec![body],
        Term::Let(_, _, bound, body) => vec![bound, body],
        Term::Case(scrutinee, _, left, _, right) => vec![scrutinee, left, right],
        Term::If(c, t, e) => vec![c, t, e],
        Term::AdtConstruct(_, _, payload) => vec![payload],
        Term::ExternCall(_, args) => args.iter().collect(),
        Term::AdtMatch(scrutinee, arms) => {
            let mut out = vec![scrutinee.as_ref()];
            out.extend(arms.iter().map(|(_, _, body)| body.as_ref()));
            out
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests;
