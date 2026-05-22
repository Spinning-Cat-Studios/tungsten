//! Recursion classification for individual functions.
//!
//! Classifies recursive functions into:
//! - TailRecursive: all recursive calls in tail position
//! - TreeRecursive: multiple recursive calls per branch
//! - LinearNonTail: single recursive call per branch, not in tail position
//! - General: everything else

use tungsten_core::terms::Term;

/// The kind of recursion detected for a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecursionKind {
    /// All recursive calls are in tail position — musttail eligible.
    TailRecursive,
    /// Multiple recursive calls per branch — stack depth = O(tree height).
    TreeRecursive,
    /// Single non-tail recursive call per branch — stack depth = O(n).
    LinearNonTail,
    /// Does not fit the above categories — needs manual review.
    General,
}

impl std::fmt::Display for RecursionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TailRecursive => write!(f, "TAIL-RECURSIVE"),
            Self::TreeRecursive => write!(f, "TREE-RECURSIVE"),
            Self::LinearNonTail => write!(f, "LINEAR NON-TAIL"),
            Self::General => write!(f, "GENERAL"),
        }
    }
}

/// Classify the recursion pattern of a function.
///
/// `fn_name` is the function's own name (used to detect self-references).
/// `term` is the function's body.
///
/// For Fix bindings, the fix variable name is used instead of fn_name.
pub fn classify_recursion(fn_name: &str, term: &Term) -> RecursionKind {
    // Unwrap outer lambdas and fix to find the actual body + recursion variable
    let (rec_var, body) = unwrap_function(fn_name, term);

    // Count recursive calls in tail vs non-tail positions
    let mut info = RecursionInfo::default();
    analyze_calls(&rec_var, body, true, &mut info);

    if info.total_calls == 0 {
        // No recursive calls found (shouldn't happen if SCC said recursive,
        // but handle gracefully)
        return RecursionKind::General;
    }

    if info.non_tail_calls == 0 {
        // All calls are in tail position
        RecursionKind::TailRecursive
    } else if info.max_calls_per_branch > 1 {
        // Multiple recursive calls in at least one branch
        RecursionKind::TreeRecursive
    } else if info.non_tail_calls > 0 && info.max_calls_per_branch <= 1 {
        RecursionKind::LinearNonTail
    } else {
        RecursionKind::General
    }
}

/// Unwrap outer Fix/Lambda wrappers to find the recursion variable and body.
fn unwrap_function<'a>(fn_name: &str, term: &'a Term) -> (String, &'a Term) {
    match term {
        Term::Fix(fix_var, _, body) => {
            // The fix variable is the recursion variable
            let inner = unwrap_lambdas(body);
            (fix_var.clone(), inner)
        }
        Term::Lambda(_, _, body) => unwrap_function(fn_name, body),
        Term::Spanned(inner, _) => unwrap_function(fn_name, inner),
        _ => (fn_name.to_string(), term),
    }
}

/// Unwrap nested Lambda wrappers to get to the body.
fn unwrap_lambdas(term: &Term) -> &Term {
    match term {
        Term::Lambda(_, _, body) => unwrap_lambdas(body),
        Term::Spanned(inner, _) => unwrap_lambdas(inner),
        _ => term,
    }
}

#[derive(Default)]
struct RecursionInfo {
    /// Total recursive calls found
    total_calls: usize,
    /// Calls not in tail position
    non_tail_calls: usize,
    /// Maximum recursive calls in any single branch
    max_calls_per_branch: usize,
}

/// Analyze recursive calls in a term, tracking tail position.
fn analyze_calls(rec_var: &str, term: &Term, in_tail: bool, info: &mut RecursionInfo) {
    match term {
        // Direct or curried recursive call: App(f, _) or App(App(f, _), _)
        Term::App(..) => {
            analyze_app(rec_var, term, in_tail, info);
        }

        // Let: body is in tail if let is in tail, def is never tail
        Term::Let(_, _, def, body) => {
            analyze_calls(rec_var, def, false, info);
            analyze_calls(rec_var, body, in_tail, info);
        }

        // If: both branches inherit tail position, condition is not tail
        Term::If(cond, then_br, else_br) => {
            analyze_calls(rec_var, cond, false, info);
            analyze_two_branches(rec_var, then_br, else_br, in_tail, info);
        }

        // Case: both arms inherit tail position, scrutinee is not tail
        Term::Case(scrut, _, left, _, right) => {
            analyze_calls(rec_var, scrut, false, info);
            analyze_two_branches(rec_var, left, right, in_tail, info);
        }

        // AdtMatch: all arms inherit tail position, scrutinee is not tail
        Term::AdtMatch(scrut, arms) => {
            analyze_calls(rec_var, scrut, false, info);
            analyze_match_arms(rec_var, arms, in_tail, info);
        }

        // Spanned: transparent — preserve tail position
        Term::Spanned(inner, _) => analyze_calls(rec_var, inner, in_tail, info),

        // Everything else: children are never in tail position
        _ => {
            term.for_each_subterm(|child| analyze_calls(rec_var, child, false, info));
        }
    }
}

/// Analyze an application term for recursive calls.
///
/// `app` must be a `Term::App(func, arg)`.
fn analyze_app(rec_var: &str, app: &Term, in_tail: bool, info: &mut RecursionInfo) {
    let (func, arg) = match app {
        Term::App(f, a) => (f.as_ref(), a.as_ref()),
        _ => return,
    };
    let is_rec_call = is_recursive_ref(rec_var, func);
    let is_curried_rec = !is_rec_call && is_curried_recursive_call(rec_var, app);

    if is_rec_call {
        info.total_calls += 1;
        if !in_tail {
            info.non_tail_calls += 1;
        }
        analyze_calls(rec_var, arg, false, info);
    } else if is_curried_rec {
        info.total_calls += 1;
        if !in_tail {
            info.non_tail_calls += 1;
        }
        collect_curried_args(rec_var, app, info);
    } else {
        analyze_calls(rec_var, func, false, info);
        analyze_calls(rec_var, arg, false, info);
    }
}

/// Analyze two branches (If/Case) and merge results into info.
fn analyze_two_branches(
    rec_var: &str,
    left: &Term,
    right: &Term,
    in_tail: bool,
    info: &mut RecursionInfo,
) {
    let mut left_info = RecursionInfo::default();
    let mut right_info = RecursionInfo::default();
    analyze_calls(rec_var, left, in_tail, &mut left_info);
    analyze_calls(rec_var, right, in_tail, &mut right_info);

    info.total_calls += left_info.total_calls + right_info.total_calls;
    info.non_tail_calls += left_info.non_tail_calls + right_info.non_tail_calls;
    let branch_max = left_info.total_calls.max(right_info.total_calls);
    info.max_calls_per_branch = info.max_calls_per_branch.max(branch_max);
}

/// Analyze AdtMatch arms and merge results into info.
fn analyze_match_arms(
    rec_var: &str,
    arms: &[(usize, String, Box<Term>)],
    in_tail: bool,
    info: &mut RecursionInfo,
) {
    let mut max_branch_calls = 0usize;
    for (_, _, body) in arms {
        let mut arm_info = RecursionInfo::default();
        analyze_calls(rec_var, body, in_tail, &mut arm_info);
        info.total_calls += arm_info.total_calls;
        info.non_tail_calls += arm_info.non_tail_calls;
        max_branch_calls = max_branch_calls.max(arm_info.total_calls);
    }
    info.max_calls_per_branch = info.max_calls_per_branch.max(max_branch_calls);
}

/// Check if a term is a reference to the recursive variable.
/// Check if an application chain has the recursive ref at its root.
/// E.g. App(App(f, x), y) where f is the recursive ref.
fn is_curried_recursive_call(rec_var: &str, term: &Term) -> bool {
    match term {
        Term::App(func, _) => {
            is_recursive_ref(rec_var, func) || is_curried_recursive_call(rec_var, func)
        }
        Term::Spanned(inner, _) => is_curried_recursive_call(rec_var, inner),
        _ => false,
    }
}

/// Collect and analyze arguments in a curried application chain,
/// without counting the recursive call itself again.
fn collect_curried_args(rec_var: &str, term: &Term, info: &mut RecursionInfo) {
    match term {
        Term::App(func, arg) => {
            analyze_calls(rec_var, arg, false, info);
            if !is_recursive_ref(rec_var, func) {
                collect_curried_args(rec_var, func, info);
            }
        }
        Term::Spanned(inner, _) => collect_curried_args(rec_var, inner, info),
        _ => {}
    }
}

fn is_recursive_ref(rec_var: &str, term: &Term) -> bool {
    match term {
        Term::Var(name) | Term::Global(name) => name == rec_var,
        Term::TyApp(inner, _) => is_recursive_ref(rec_var, inner),
        Term::Spanned(inner, _) => is_recursive_ref(rec_var, inner),
        _ => false,
    }
}
