//! The size environment and the strict-subterm relation
//! (ADR 29.6.26e §2.1).
//!
//! While checking a function body the environment records, for each in-scope
//! variable, how its size relates to the **root** — the decreasing parameter
//! chosen for that function. Only two classes matter in Phase 1: a variable
//! that *is* the root (or an alias of it), and one destructuring has proved
//! strictly smaller.
//!
//! Reconstruction is deliberately not descent: `Succ(n)` is not a strict
//! subterm of `Succ(n)`, so [`classify`] answers `None` for every constructor
//! form. Only variables bound by destructuring — and their aliases and
//! projections — are ever [`SizeClass::Smaller`].

use std::collections::HashMap;

use crate::terms::{Term, TermSpan, Var};

use super::graph::{peel_spine, transparent};

/// How a term's size relates to the decreasing root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeClass {
    /// The root itself, or something the same size (a μ-unfold, an alias).
    SameAsRoot,
    /// Proved strictly smaller by destructuring.
    Smaller,
}

/// Variables in scope, keyed to their size class. Absent means "unrelated to
/// the root" — which is also what shadowing produces.
pub type SizeEnv = HashMap<Var, SizeClass>;

/// One call to a member of the caller's own recursive group.
#[derive(Debug, Clone)]
pub struct CallSite {
    /// The group member being called.
    pub callee: String,
    /// Value arguments, rendered for diagnostics (positional, spine order).
    pub arguments: Vec<String>,
    /// Argument positions proved strictly smaller than the root.
    pub smaller: Vec<usize>,
    /// Where to point the diagnostic.
    pub span: Option<TermSpan>,
}

impl CallSite {
    /// Whether this call supplies a strictly smaller argument at `position`.
    #[must_use]
    pub fn descends_at(&self, position: usize) -> bool {
        self.smaller.contains(&position)
    }

    /// Whether the spine is long enough to supply an argument at `position`.
    ///
    /// An under-applied call leaves the callee as a closure rather than
    /// consuming its decreasing argument, so it can never be a descent.
    #[must_use]
    pub fn supplies(&self, position: usize) -> bool {
        position < self.arguments.len()
    }
}

/// How a term's size relates to the root under `env`.
///
/// `None` means "no relation established" — the conservative answer, and the
/// one every constructor, literal, call result and opaque expression gets.
#[must_use]
pub fn classify(term: &Term, env: &SizeEnv) -> Option<SizeClass> {
    match transparent(term) {
        Term::Var(name) => env.get(name).copied(),
        // Unfolding a μ exposes the same value at a different type.
        Term::Unfold(_, inner) => classify(inner, env),
        // A projection of anything at-most-root-sized is strictly smaller.
        Term::Fst(inner) | Term::Snd(inner) => classify(inner, env).map(|_| SizeClass::Smaller),
        _ => None,
    }
}

/// Walk `body` under `root`, collecting every call to a member of `group`.
///
/// `root` is the decreasing parameter's name. The walk is re-run once per
/// candidate root, which is why it returns data rather than a verdict: the
/// choice of root is made afterwards, over all candidates at once.
#[must_use]
pub fn collect_call_sites(body: &Term, root: &str, group: &[String]) -> Vec<CallSite> {
    let mut env: SizeEnv = HashMap::new();
    env.insert(root.to_string(), SizeClass::SameAsRoot);
    let mut sites = Vec::new();
    walk(body, &env, group, None, &mut sites);
    sites
}

/// Recursive worker for [`collect_call_sites`].
///
/// `span` is the innermost enclosing `Spanned` wrapper, threaded so a call site
/// can be pointed at even though `App` itself carries no span.
fn walk(
    term: &Term,
    env: &SizeEnv,
    group: &[String],
    span: Option<TermSpan>,
    out: &mut Vec<CallSite>,
) {
    match term {
        Term::Spanned(inner, inner_span) => walk(inner, env, group, Some(*inner_span), out),
        Term::Annot(inner, _) => walk(inner, env, group, span, out),

        Term::Lambda(binder, _, inner) => {
            walk(inner, &shadowed(env, binder), group, span, out);
        }

        Term::Fix(binder, _, inner) => {
            walk(inner, &shadowed(env, binder), group, span, out);
        }

        Term::Let(binder, _, rhs, inner) => {
            walk(rhs, env, group, span, out);
            let mut extended = shadowed(env, binder);
            if let Some(class) = classify(rhs, env) {
                extended.insert(binder.clone(), class);
            }
            walk(inner, &extended, group, span, out);
        }

        // Destructuring a term at most the root's size binds its payload, and a
        // payload is a proper component of what it was taken out of.
        Term::Case(scrutinee, left_binder, left, right_binder, right) => {
            walk(scrutinee, env, group, span, out);
            let destructured = classify(scrutinee, env).is_some();
            for (binder, arm) in [(left_binder, left), (right_binder, right)] {
                walk(
                    arm,
                    &bound_payload(env, binder, destructured),
                    group,
                    span,
                    out,
                );
            }
        }

        Term::AdtMatch(scrutinee, arms) => {
            walk(scrutinee, env, group, span, out);
            let destructured = classify(scrutinee, env).is_some();
            for (_, binder, arm) in arms {
                walk(
                    arm,
                    &bound_payload(env, binder, destructured),
                    group,
                    span,
                    out,
                );
            }
        }

        Term::App(..) | Term::TyApp(..) => walk_application(term, env, group, span, out),

        other => other.for_each_subterm(|child| walk(child, env, group, span, out)),
    }
}

/// Handle an application spine: record it when its head is a group member, then
/// walk the arguments.
fn walk_application(
    term: &Term,
    env: &SizeEnv,
    group: &[String],
    span: Option<TermSpan>,
    out: &mut Vec<CallSite>,
) {
    let (head, args) = peel_spine(term);
    match head {
        Term::Global(callee) if group.iter().any(|member| member == callee) && !args.is_empty() => {
            out.push(CallSite {
                callee: callee.clone(),
                arguments: args
                    .iter()
                    .map(|arg| transparent(arg).to_string())
                    .collect(),
                smaller: args
                    .iter()
                    .enumerate()
                    .filter(|(_, arg)| classify(arg, env) == Some(SizeClass::Smaller))
                    .map(|(index, _)| index)
                    .collect(),
                span,
            });
        }
        other => walk(other, env, group, span, out),
    }
    for arg in args {
        walk(arg, env, group, span, out);
    }
}

/// `env` with `binder` shadowed — a rebound name loses whatever the outer one
/// had proved.
fn shadowed(env: &SizeEnv, binder: &str) -> SizeEnv {
    let mut cloned = env.clone();
    cloned.remove(binder);
    cloned
}

/// `env` extended with a match arm's payload binder, marked strictly smaller
/// when the scrutinee was related to the root at all.
fn bound_payload(env: &SizeEnv, binder: &str, destructured: bool) -> SizeEnv {
    let mut extended = shadowed(env, binder);
    if destructured {
        extended.insert(binder.to_string(), SizeClass::Smaller);
    }
    extended
}
