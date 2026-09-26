//! Integer `match` — a `match` on a `Nat` or `Int` scrutinee (ADR 18.9.26e).
//!
//! No new Core node: the arms lower to a chain of conditionals over the
//! existing equality primitives, `NatEq` for a `Nat` scrutinee and
//! `IntBin(Eq)` for an `Int` one, against a let-bound scrutinee:
//!
//! ```text
//! let s = scrutinee in
//!   if c₁ then b₁ else if c₂ then b₂ … else catch_all
//! ```
//!
//! An or-of-literals tests `if eq₁ then true else eq₂`; a guard is
//! `if test then guard else false`; a variable arm binds the scrutinee for
//! its own guard and body only, so it never shadows a name a later arm uses.
//! Literal patterns cannot cover every integer, so exactly one *unguarded*
//! catch-all (`x` or `_`) is required — it is the chain's floor. LLVM's
//! SimplifyCFG turns the chain into a `switch`.
//!
//! Unreachable arms — a duplicate literal, anything after the catch-all —
//! are W0001 through `warn`, the way the ADT classifier raises them, so the
//! file still compiles.

use std::collections::HashMap;

use crate::ast::{self, LiteralPattern, Pattern};
use crate::span::{Span, Spanned};
use tungsten_core::terms::IntBinOp;
use tungsten_core::{Term, Type};

use super::forms::{int_literal_out_of_range, int_literal_value};
use crate::elaborate::error::{ElabError, ElabErrorKind, ExpectedContext};
use crate::elaborate::{ElabResult, Elaborator};

#[cfg(test)]
mod tests;

/// A literal on the scrutinee's own number type — the key duplicates are
/// found by, so `-0` and `0` on an `Int` are one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NumericLiteral {
    Nat(u64),
    Int(i64),
}

impl NumericLiteral {
    /// `scrutinee == self`, on the scrutinee's own equality primitive.
    fn equals(self, scrutinee: Term) -> Term {
        match self {
            NumericLiteral::Nat(n) => Term::nat_eq(scrutinee, Term::nat_smart(n)),
            NumericLiteral::Int(v) => Term::int_bin(IntBinOp::Eq, scrutinee, Term::int_lit(v)),
        }
    }
}

/// What an arm's pattern tests, before its guard.
#[derive(Debug, PartialEq)]
enum ArmTest {
    /// The scrutinee equals one of these — one literal, or an or-of-literals.
    OneOf(Vec<(NumericLiteral, Span)>),
    /// Matches every value: `x` binds the scrutinee, `_` does not.
    CatchAll(Option<String>),
}

/// The arms that can run, in source order — the last is the unguarded
/// catch-all — and the warnings for those that cannot.
#[derive(Debug)]
struct IntArmPlan {
    tests: Vec<ArmTest>,
    warnings: Vec<ElabError>,
}

/// One arm above the catch-all, elaborated.
struct ElaboratedArm {
    test: ArmTest,
    guard: Option<Term>,
    body: Term,
}

fn unsupported_pattern(span: Span, what: &str) -> ElabError {
    ElabError::new(span, ElabErrorKind::UnsupportedPattern(what.to_string()))
}

/// A literal pattern as a value of the scrutinee's number type.
fn numeric_literal(literal: &LiteralPattern, is_int: bool) -> ElabResult<NumericLiteral> {
    match (literal, is_int) {
        (LiteralPattern::Int(n, _), false) => Ok(NumericLiteral::Nat(*n)),
        (LiteralPattern::Int(n, span), true) => int_literal_value(*n, false)
            .map(NumericLiteral::Int)
            .ok_or_else(|| int_literal_out_of_range(*n, false, *span)),
        (LiteralPattern::NegInt(n, span), true) => int_literal_value(*n, true)
            .map(NumericLiteral::Int)
            .ok_or_else(|| int_literal_out_of_range(*n, true, *span)),
        (LiteralPattern::NegInt(_, span), false) => Err(unsupported_pattern(
            *span,
            "negative literal on a Nat scrutinee",
        )
        .with_help(
            "a `Nat` is never negative; match on an `Int` instead — `match to_int(n) { … }`",
        )),
        (other, _) => Err(unsupported_pattern(
            other.span(),
            "non-integer literal in an integer match",
        )),
    }
}

/// Flatten an or-pattern into its literals; any other alternative refuses.
fn collect_or_literals(
    pattern: &Pattern,
    is_int: bool,
    out: &mut Vec<(NumericLiteral, Span)>,
) -> ElabResult<()> {
    match pattern {
        Pattern::Or(left, right, _) => {
            collect_or_literals(left, is_int, out)?;
            collect_or_literals(right, is_int, out)
        }
        Pattern::Literal(literal) => {
            out.push((numeric_literal(literal, is_int)?, literal.span()));
            Ok(())
        }
        Pattern::Var(_) | Pattern::Wildcard(_) => Err(unsupported_pattern(
            pattern.span(),
            "variable or wildcard inside an or-pattern",
        )
        .with_help("it matches every value, so the or-pattern is a catch-all in disguise; give it an arm of its own")),
        other => Err(unsupported_pattern(
            other.span(),
            "non-literal alternative in an integer or-pattern",
        )),
    }
}

/// Classify one arm's pattern.
fn classify_pattern(pattern: &Pattern, is_int: bool) -> ElabResult<ArmTest> {
    match pattern {
        Pattern::Wildcard(_) => Ok(ArmTest::CatchAll(None)),
        Pattern::Var(ident) => Ok(ArmTest::CatchAll(Some(ident.name.clone()))),
        Pattern::Literal(literal) => Ok(ArmTest::OneOf(vec![(
            numeric_literal(literal, is_int)?,
            literal.span(),
        )])),
        Pattern::Or(..) => {
            let mut literals = Vec::new();
            collect_or_literals(pattern, is_int, &mut literals)?;
            Ok(ArmTest::OneOf(literals))
        }
        other => Err(unsupported_pattern(
            other.span(),
            "constructor or tuple pattern in an integer match",
        )
        .with_help("an integer match takes literal, variable and wildcard patterns")),
    }
}

/// W0001 for a literal an earlier unguarded arm already matches.
fn duplicate_literal_warnings(
    literals: &[(NumericLiteral, Span)],
    first_seen: &HashMap<NumericLiteral, Span>,
) -> Vec<ElabError> {
    literals
        .iter()
        .filter_map(|(literal, span)| {
            let first = first_seen.get(literal)?;
            Some(
                ElabError::new(*span, ElabErrorKind::UnreachableArm)
                    .with_span_note(*first, "an earlier arm already matches this value")
                    .with_help("remove the duplicate literal"),
            )
        })
        .collect()
}

/// Classify every arm, find the catch-all, and warn about what cannot run.
///
/// Pure over the AST, so exhaustiveness and reachability are tested without
/// elaborating a body.
fn plan_int_arms(arms: &[ast::MatchArm], is_int: bool, span: Span) -> ElabResult<IntArmPlan> {
    let mut tests = Vec::new();
    let mut warnings = Vec::new();
    let mut first_seen: HashMap<NumericLiteral, Span> = HashMap::new();
    let mut catch_all_span: Option<Span> = None;

    for arm in arms {
        let test = classify_pattern(&arm.pattern, is_int)?;
        if let Some(catch_span) = catch_all_span {
            warnings.push(
                ElabError::new(arm.pattern.span(), ElabErrorKind::UnreachableArm)
                    .with_span_note(catch_span, "this catch-all pattern matches all values")
                    .with_help("remove this arm or move the catch-all pattern to the end"),
            );
            continue;
        }
        if let ArmTest::OneOf(literals) = &test {
            warnings.extend(duplicate_literal_warnings(literals, &first_seen));
            if arm.guard.is_none() {
                for (literal, literal_span) in literals {
                    first_seen.entry(*literal).or_insert(*literal_span);
                }
            }
        }
        if arm.guard.is_none() && matches!(test, ArmTest::CatchAll(_)) {
            catch_all_span = Some(arm.pattern.span());
        }
        tests.push(test);
    }

    if catch_all_span.is_none() {
        return Err(
            ElabError::new(span, ElabErrorKind::NonExhaustiveMatch).with_help(
                "literal patterns cannot cover every integer; add an unguarded `_ =>` arm",
            ),
        );
    }
    Ok(IntArmPlan { tests, warnings })
}

/// `scrutinee == l₁ || scrutinee == l₂ …`, as `if eq₁ then true else …`.
fn any_literal_equals(scrutinee_name: &str, literals: &[(NumericLiteral, Span)]) -> Term {
    literals
        .iter()
        .rev()
        .fold(None, |rest, (literal, _)| {
            let equality = literal.equals(Term::var(scrutinee_name));
            Some(match rest {
                None => equality,
                Some(rest) => Term::if_then_else(equality, Term::True, rest),
            })
        })
        .unwrap_or(Term::False)
}

/// An arm's full condition: its pattern test, then its guard, short-circuit.
fn arm_condition(scrutinee_name: &str, test: &ArmTest, guard: Option<Term>) -> Term {
    let pattern_test = match test {
        ArmTest::OneOf(literals) => Some(any_literal_equals(scrutinee_name, literals)),
        ArmTest::CatchAll(_) => None,
    };
    match (pattern_test, guard) {
        (Some(test), Some(guard)) => Term::if_then_else(test, guard, Term::False),
        (Some(test), None) => test,
        (None, Some(guard)) => guard,
        // Defensive: an unguarded catch-all is the floor, never a chain link.
        (None, None) => Term::True,
    }
}

/// Fold the arms back-to-front onto the catch-all's body.
fn build_int_match_chain(scrutinee_name: &str, arms: Vec<ElaboratedArm>, floor: Term) -> Term {
    arms.into_iter().rev().fold(floor, |rest, arm| {
        let condition = arm_condition(scrutinee_name, &arm.test, arm.guard);
        Term::if_then_else(condition, arm.body, rest)
    })
}

/// The scrutinee and result types every arm shares.
struct ArmTypes<'t> {
    scrutinee_name: &'t str,
    scrutinee_ty: &'t Type,
    result_ty: Option<Type>,
    first_body: Option<Span>,
}

impl<'a> Elaborator<'a> {
    /// Elaborate a `match` whose scrutinee is a `Nat` or an `Int`.
    pub(super) fn elab_int_match(
        &mut self,
        scrutinee: Term,
        scrutinee_ty: Type,
        arms: &[ast::MatchArm],
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        let plan = plan_int_arms(arms, scrutinee_ty == Type::Int, span)?;
        for warning in plan.warnings {
            self.warn(warning);
        }
        let scrutinee_name = self.fresh_var("int_scrut");
        let mut types = ArmTypes {
            scrutinee_name: &scrutinee_name,
            scrutinee_ty: &scrutinee_ty,
            result_ty: expected.cloned(),
            first_body: None,
        };
        let mut tests = plan.tests;
        // `plan_int_arms` refuses a plan without its catch-all, so this pops it.
        let floor_test = tests.pop();
        let mut elaborated = Vec::with_capacity(tests.len());
        for (arm, test) in arms.iter().zip(tests) {
            let guard = match &arm.guard {
                Some(guard) => Some(self.elab_int_arm_part(&test, guard, &mut types, true)?),
                None => None,
            };
            let body = self.elab_int_arm_part(&test, &arm.body, &mut types, false)?;
            elaborated.push(ElaboratedArm { test, guard, body });
        }
        let floor_test = floor_test.unwrap_or(ArmTest::CatchAll(None));
        let floor =
            self.elab_int_arm_part(&floor_test, &arms[elaborated.len()].body, &mut types, false)?;

        let chain = build_int_match_chain(&scrutinee_name, elaborated, floor);
        let result_ty = types.result_ty.unwrap_or(Type::Unit);
        Ok((
            Term::let_in(scrutinee_name, scrutinee_ty, scrutinee, chain),
            result_ty,
        ))
    }

    /// Elaborate an arm's guard (against `Bool`) or body (against the
    /// match's result type, fixed by the first body when nothing is expected),
    /// in a scope binding the arm's variable, if it has one.
    fn elab_int_arm_part(
        &mut self,
        test: &ArmTest,
        expr: &ast::Expr,
        types: &mut ArmTypes<'_>,
        is_guard: bool,
    ) -> ElabResult<Term> {
        let ArmTest::CatchAll(Some(binder)) = test else {
            return self.elab_int_arm_expr(expr, types, is_guard);
        };
        let scrutinee_ty = types.scrutinee_ty.clone();
        let term = self.with_scoped_binding(binder, scrutinee_ty.clone(), |elab| {
            elab.elab_int_arm_expr(expr, types, is_guard)
        })?;
        Ok(Term::let_in(
            binder.clone(),
            scrutinee_ty,
            Term::var(types.scrutinee_name),
            term,
        ))
    }

    fn elab_int_arm_expr(
        &mut self,
        expr: &ast::Expr,
        types: &mut ArmTypes<'_>,
        is_guard: bool,
    ) -> ElabResult<Term> {
        if is_guard {
            return self.check(expr, &Type::Bool);
        }
        let Some(result_ty) = types.result_ty.clone() else {
            let (term, ty) = self.infer(expr)?;
            types.result_ty = Some(ty);
            types.first_body = Some(expr.span());
            return Ok(term);
        };
        let Some(first_body) = types.first_body else {
            return self.check(expr, &result_ty);
        };
        self.push_context(ExpectedContext::branch_unification(first_body));
        let term = self.check(expr, &result_ty);
        self.pop_context();
        term
    }
}
