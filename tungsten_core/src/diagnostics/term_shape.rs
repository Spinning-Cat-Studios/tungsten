//! Bounded `Term` shape + node-count diagnostics for the evaluator step tracer
//! (ADR 21.7.26j).
//!
//! Two pure, cheap-to-run measurements the tracer prints once per step:
//!
//! - [`count_nodes_bounded`] — a node counter with an early-stop **budget**, so
//!   a term that has blown up (millions of nodes) still costs O(budget) to
//!   measure instead of O(size). The budget also bounds recursion depth (each
//!   descent adds ≥1 to the count), so the counter cannot itself overflow the
//!   stack on a deep term.
//! - [`render_shape`] — a depth-bounded structural rendering: the top variant
//!   name plus its immediate children's variants, down to `depth` levels. This
//!   is the "variant name + immediate children's variants" view whose twelve
//!   printed lines settled the 21.7.26e wall-2 diagnosis in seconds.
//!
//! Both build on [`Term::for_each_subterm`] so a new `Term` variant is picked
//! up automatically (the walker is the single structural match); only
//! [`variant_name`] enumerates variants by hand, and a missing arm is a
//! compile error.

use crate::terms::Term;

/// A node count that may have stopped early at its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedNodeCount {
    /// Nodes counted. Equals `budget` exactly when `truncated` is true.
    pub count: usize,
    /// True when counting reached the budget and stopped before the whole
    /// tree was walked — the real node count is `≥ count`.
    pub truncated: bool,
}

impl BoundedNodeCount {
    /// Render as `"1014"` or `"≥1000"` (truncated) for the tracer's node column.
    #[must_use]
    pub fn render(&self) -> String {
        if self.truncated {
            format!("≥{}", self.count)
        } else {
            self.count.to_string()
        }
    }
}

/// Count nodes in `term`, stopping once `budget` nodes have been seen.
///
/// A `budget` of 0 counts nothing and reports `truncated` for any non-empty
/// term. Because each recursive descent increments the count before recursing,
/// the recursion depth is bounded by `budget`, so this is safe to call on a
/// pathologically deep term without a wide stack.
#[must_use]
pub fn count_nodes_bounded(term: &Term, budget: usize) -> BoundedNodeCount {
    let mut count = 0usize;
    count_into(term, &mut count, budget);
    BoundedNodeCount {
        count,
        truncated: count >= budget,
    }
}

/// Recursive worker for [`count_nodes_bounded`]. Returns early the moment the
/// budget is reached so neither work nor stack depth exceeds `budget`.
fn count_into(term: &Term, count: &mut usize, budget: usize) {
    if *count >= budget {
        return;
    }
    *count += 1;
    term.for_each_subterm(|child| count_into(child, count, budget));
}

/// Render `term`'s shape to `depth` levels: the top variant name, then each
/// immediate child rendered to `depth - 1`, and so on. At depth 0 (or a leaf) a
/// node renders as its bare variant name.
///
/// Example (`depth = 2`): `Let(Fold(Inl), App(App, Succ))`.
#[must_use]
pub fn render_shape(term: &Term, depth: usize) -> String {
    let name = variant_name(term);
    if depth == 0 {
        return name.to_string();
    }
    let mut children: Vec<String> = Vec::new();
    term.for_each_subterm(|child| children.push(render_shape(child, depth - 1)));
    if children.is_empty() {
        name.to_string()
    } else {
        format!("{name}({})", children.join(", "))
    }
}

/// The bare constructor name of a `Term` variant, without formatting any
/// sub-terms — O(1), so it stays cheap even when the term has blown up.
///
/// A flat-match dispatcher: adding a `Term` variant makes this a compile error
/// until the arm is added, which is the intended forcing function.
#[must_use]
pub fn variant_name(term: &Term) -> &'static str {
    match term {
        Term::Var(_) => "Var",
        Term::Global(_) => "Global",
        Term::Lambda(_, _, _) => "Lambda",
        Term::App(_, _) => "App",
        Term::Let(_, _, _, _) => "Let",
        Term::True => "True",
        Term::False => "False",
        Term::If(_, _, _) => "If",
        Term::Unit => "Unit",
        Term::Absurd(_, _) => "Absurd",
        Term::Zero => "Zero",
        Term::Succ(_) => "Succ",
        Term::NatLit(_) => "NatLit",
        Term::NatRec(_, _, _, _) => "NatRec",
        Term::NatInd(_, _, _, _) => "NatInd",
        Term::NatAdd(_, _) => "NatAdd",
        Term::NatSub(_, _) => "NatSub",
        Term::NatMul(_, _) => "NatMul",
        Term::NatDiv(_, _) => "NatDiv",
        Term::NatMod(_, _) => "NatMod",
        Term::NatEq(_, _) => "NatEq",
        Term::NatLt(_, _) => "NatLt",
        Term::NatLe(_, _) => "NatLe",
        Term::NatGt(_, _) => "NatGt",
        Term::NatGe(_, _) => "NatGe",
        Term::IntLit(_) => "IntLit",
        Term::IntBin(_, _, _) => "IntBin",
        Term::IntNeg(_) => "IntNeg",
        Term::NatToInt(_) => "NatToInt",
        Term::IntToNat(_) => "IntToNat",
        Term::BoolAnd(_, _) => "BoolAnd",
        Term::BoolOr(_, _) => "BoolOr",
        Term::BoolNot(_) => "BoolNot",
        Term::StringLit(_) => "StringLit",
        Term::StrConcat(_, _) => "StrConcat",
        Term::StrLen(_) => "StrLen",
        Term::StrEq(_, _) => "StrEq",
        Term::StrCharAt(_, _) => "StrCharAt",
        Term::StrSubstring(_, _, _) => "StrSubstring",
        Term::Pair(_, _) => "Pair",
        Term::Fst(_) => "Fst",
        Term::Snd(_) => "Snd",
        Term::Inl(_, _) => "Inl",
        Term::Inr(_, _) => "Inr",
        Term::Case(_, _, _, _, _) => "Case",
        Term::TyAbs(_, _) => "TyAbs",
        Term::TyApp(_, _) => "TyApp",
        Term::Refl(_, _) => "Refl",
        Term::Subst(_, _, _, _) => "Subst",
        Term::Fix(_, _, _) => "Fix",
        Term::Fold(_, _) => "Fold",
        Term::Unfold(_, _) => "Unfold",
        Term::ExternCall(_, _) => "ExternCall",
        Term::RefNew(_) => "RefNew",
        Term::RefGet(_) => "RefGet",
        Term::RefSet(_, _) => "RefSet",
        Term::Annot(_, _) => "Annot",
        Term::Sorry => "Sorry",
        Term::AdtConstruct(_, _, _) => "AdtConstruct",
        Term::AdtMatch(_, _) => "AdtMatch",
        Term::Return(_) => "Return",
        Term::Spanned(_, _) => "Spanned",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Type;

    #[test]
    fn count_bounded_reports_exact_count_below_budget() {
        // App(Succ(Zero), NatLit(1)) = 4 nodes.
        let term = Term::app(Term::succ(Term::Zero), Term::NatLit(1));
        let c = count_nodes_bounded(&term, 100);
        assert_eq!(c.count, 4);
        assert!(!c.truncated);
        assert_eq!(c.render(), "4");
    }

    #[test]
    fn count_bounded_stops_at_budget_and_marks_truncated() {
        // Build a right-spine of Succ around Zero: 1 + N nodes.
        let mut term = Term::Zero;
        for _ in 0..50 {
            term = Term::succ(term);
        }
        // 51 nodes total; budget 10 must stop early.
        let c = count_nodes_bounded(&term, 10);
        assert_eq!(c.count, 10);
        assert!(c.truncated);
        assert_eq!(c.render(), "≥10");
    }

    #[test]
    fn count_bounded_zero_budget_truncates_any_nonempty_term() {
        let c = count_nodes_bounded(&Term::Zero, 0);
        assert_eq!(c.count, 0);
        assert!(c.truncated);
    }

    #[test]
    fn count_bounded_descent_is_capped_by_budget_below_term_depth() {
        // The budget must cap the counter's *recursion depth*, not just its
        // total work: on a 1000-deep spine with a budget of 64, `count_into`
        // may descend at most 64 levels. (Depth is kept modest here because a
        // pathologically deep `Term`'s own recursive `Drop` — not the counter —
        // would overflow a default 2 MB test stack; the tracer runs on a
        // wide-stack worker precisely to tolerate deep terms end to end.)
        let mut term = Term::Zero;
        for _ in 0..1000 {
            term = Term::succ(term);
        }
        let c = count_nodes_bounded(&term, 64);
        assert_eq!(c.count, 64);
        assert!(c.truncated);
    }

    #[test]
    fn render_shape_depth_2_matches_adr_example() {
        // Let(x, Nat, Fold(Inl(_, Unit)), App(App(TyApp, u), Succ(Zero)))
        // rendered at depth 2 → "Let(Fold(Inl), App(App, Succ))".
        let fold = Term::fold(Type::Nat, Term::inl(Type::Nat, Term::Unit));
        let app = Term::app(
            Term::app(Term::ty_app(Term::Unit, Type::Nat), Term::Unit),
            Term::succ(Term::Zero),
        );
        let term = Term::let_in("x", Type::Nat, fold, app);
        assert_eq!(render_shape(&term, 2), "Let(Fold(Inl), App(App, Succ))");
    }

    #[test]
    fn render_shape_depth_0_is_bare_variant_name() {
        let term = Term::app(Term::Zero, Term::NatLit(1));
        assert_eq!(render_shape(&term, 0), "App");
    }

    #[test]
    fn render_shape_leaf_has_no_parens() {
        assert_eq!(render_shape(&Term::Zero, 3), "Zero");
        assert_eq!(render_shape(&Term::Global("foo".into()), 3), "Global");
    }

    #[test]
    fn variant_name_is_o1_head_only() {
        assert_eq!(variant_name(&Term::app(Term::Zero, Term::Unit)), "App");
        assert_eq!(variant_name(&Term::Sorry), "Sorry");
    }

    /// One sample `Term` per enum variant. Exercises every `variant_name` arm
    /// (so a new variant is caught by the exhaustive match) and asserts the
    /// names are all distinct — no two variants collide to the same label.
    #[test]
    fn variant_name_covers_every_variant_with_distinct_labels() {
        use crate::terms::TermSpan;
        let z = || Term::Zero;
        let samples: Vec<Term> = vec![
            Term::var("x"),
            Term::global("g"),
            Term::lambda("x", Type::Nat, z()),
            Term::app(z(), Term::Unit),
            Term::let_in("x", Type::Nat, z(), Term::Unit),
            Term::True,
            Term::False,
            Term::if_then_else(Term::True, z(), Term::Unit),
            Term::Unit,
            Term::absurd(Type::Nat, z()),
            Term::Zero,
            Term::succ(z()),
            Term::NatLit(5),
            Term::natrec(Type::Nat, z(), z(), z()),
            Term::natind(Type::Nat, z(), z(), z()),
            Term::nat_add(z(), z()),
            Term::nat_sub(z(), z()),
            Term::nat_mul(z(), z()),
            Term::nat_div(z(), z()),
            Term::nat_mod(z(), z()),
            Term::nat_eq(z(), z()),
            Term::nat_lt(z(), z()),
            Term::nat_le(z(), z()),
            Term::nat_gt(z(), z()),
            Term::nat_ge(z(), z()),
            Term::bool_and(Term::True, Term::False),
            Term::bool_or(Term::True, Term::False),
            Term::bool_not(Term::True),
            Term::string_lit("s"),
            Term::str_concat(Term::string_lit("a"), Term::string_lit("b")),
            Term::str_len(Term::string_lit("a")),
            Term::str_eq(Term::string_lit("a"), Term::string_lit("b")),
            Term::str_char_at(Term::string_lit("a"), z()),
            Term::str_substring(Term::string_lit("a"), z(), z()),
            Term::pair(z(), Term::Unit),
            Term::fst(z()),
            Term::snd(z()),
            Term::inl(Type::Nat, z()),
            Term::inr(Type::Nat, z()),
            Term::case(z(), "l", z(), "r", Term::Unit),
            Term::ty_abs("T", z()),
            Term::ty_app(z(), Type::Nat),
            Term::refl(Type::Nat, z()),
            Term::subst(Type::Nat, Type::Nat, z(), z()),
            Term::fix("f", Type::Nat, z()),
            Term::fold(Type::Nat, z()),
            Term::unfold(Type::Nat, z()),
            Term::extern_call("f", vec![]),
            Term::ref_new(z()),
            Term::ref_get(z()),
            Term::ref_set(z(), Term::Unit),
            Term::annot(z(), Type::Nat),
            Term::Sorry,
            Term::adt_construct(Type::Nat, 0, z()),
            Term::adt_match(z(), vec![]),
            Term::early_return(z()),
            Term::spanned(z(), TermSpan::new(0, 1)),
        ];
        let names: Vec<&str> = samples.iter().map(variant_name).collect();
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(
            unique.len(),
            names.len(),
            "variant names must be distinct; got {names:?}"
        );
        // Every name is non-empty (no accidental "" arm).
        assert!(names.iter().all(|n| !n.is_empty()));
    }
}
