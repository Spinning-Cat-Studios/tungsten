//! Display formatting for `Term`.

use std::fmt;

use super::{Term, Var};
use crate::types::Type;

impl fmt::Display for Term {
    #[allow(clippy::many_single_char_names)] // Reason: p, z, s, n are standard NatInd display variables
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Leaf constants with fixed display strings
        if let Some(name) = self.fmt_leaf_name() {
            return write!(f, "{name}");
        }
        // Infix binary operators: (t1 OP t2)
        if let Some((t1, t2, op)) = self.fmt_infix_op() {
            return write!(f, "({t1} {op} {t2})");
        }
        // Keyword binary: (keyword t1 t2)
        if let Some((kw, t1, t2)) = self.fmt_keyword_binary() {
            return write!(f, "({kw} {t1} {t2})");
        }
        // Typed unary: (keyword [ty] t)
        if let Some((kw, ty, t)) = self.fmt_typed_unary() {
            return write!(f, "({kw} [{ty}] {t})");
        }
        // Keyword unary: (keyword t)
        if let Some((kw, t)) = self.fmt_keyword_unary() {
            return write!(f, "({kw} {t})");
        }
        match self {
            // Typed ternary: (keyword [ty] a b c)
            Term::NatRec(ty, z, s, n) => write!(f, "(natrec [{ty}] {z} {s} {n})"),
            Term::NatInd(p, z, s, n) => write!(f, "(natind [{p}] {z} {s} {n})"),
            // Binding forms: (sym x:ty. body)
            Term::Lambda(x, ty, body) => write!(f, "(λ{x}:{ty}. {body})"),
            Term::Fix(x, ty, body) => write!(f, "(fix {x}:{ty}. {body})"),
            Term::Var(v) => write!(f, "{v}"),
            Term::Global(name) => write!(f, "global:{name}"),
            Term::App(t1, t2) => write!(f, "({t1} {t2})"),
            Term::Let(x, ty, def, body) => write!(f, "(let {x} : {ty} = {def} in {body})"),
            Term::If(c, t, e) => write!(f, "(if {c} then {t} else {e})"),
            Term::NatLit(n) => write!(f, "{n}"),
            Term::StringLit(s) => write!(f, "\"{s}\""),
            Term::Pair(t1, t2) => write!(f, "({t1}, {t2})"),
            Term::Case(scrut, x, t1, y, t2) => {
                write!(f, "(case {scrut} of inl {x} => {t1} | inr {y} => {t2})")
            }
            Term::TyAbs(alpha, body) => write!(f, "(Λ{alpha}. {body})"),
            Term::TyApp(t, ty) => write!(f, "({t} [{ty}])"),
            Term::Subst(ty, p, eq, proof) => write!(f, "(subst [{ty}] [{p}] {eq} {proof})"),
            Term::Annot(t, ty) => write!(f, "({t} : {ty})"),
            Term::BoolNot(t) => write!(f, "(!{t})"),
            Term::StrSubstring(s, start, len) => write!(f, "(substring {s} {start} {len})"),
            Term::ExternCall(name, args) => fmt_extern_call(f, name, args),
            Term::AdtConstruct(adt_ty, idx, payload) => {
                write!(f, "(adt_construct [{adt_ty}] {idx} {payload})")
            }
            Term::AdtMatch(scrut, arms) => fmt_adt_match(f, scrut, arms),
            Term::Spanned(inner, _) => write!(f, "{inner}"),
            // The following arms are handled by the early-return helpers above;
            // this unreachable satisfies exhaustiveness for the compiler.
            Term::NatAdd(..)
            | Term::NatSub(..)
            | Term::NatMul(..)
            | Term::NatDiv(..)
            | Term::NatMod(..)
            | Term::NatEq(..)
            | Term::NatLt(..)
            | Term::NatLe(..)
            | Term::NatGt(..)
            | Term::NatGe(..)
            | Term::BoolAnd(..)
            | Term::BoolOr(..)
            | Term::StrConcat(..)
            | Term::StrEq(..)
            | Term::StrCharAt(..)
            | Term::RefSet(..)
            | Term::Absurd(..)
            | Term::Inl(..)
            | Term::Inr(..)
            | Term::Refl(..)
            | Term::Fold(..)
            | Term::Unfold(..)
            | Term::Succ(..)
            | Term::StrLen(..)
            | Term::Fst(..)
            | Term::Snd(..)
            | Term::RefNew(..)
            | Term::RefGet(..)
            | Term::Return(..)
            | Term::True
            | Term::False
            | Term::Unit
            | Term::Zero
            | Term::Sorry => unreachable!(),
        }
    }
}

/// Format `(extern_call name arg1 arg2 ...)`.
fn fmt_extern_call(f: &mut fmt::Formatter<'_>, name: &str, args: &[Term]) -> fmt::Result {
    write!(f, "(extern_call {name} ")?;
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            write!(f, " ")?;
        }
        write!(f, "{arg}")?;
    }
    write!(f, ")")
}

/// Format `(adt_match scrut [idx var => body | ...])`.
fn fmt_adt_match(
    f: &mut fmt::Formatter<'_>,
    scrut: &Term,
    arms: &[(usize, Var, Box<Term>)],
) -> fmt::Result {
    write!(f, "(adt_match {scrut} [")?;
    for (i, (idx, var, body)) in arms.iter().enumerate() {
        if i > 0 {
            write!(f, " | ")?;
        }
        write!(f, "{idx} {var} => {body}")?;
    }
    write!(f, "])")
}

impl Term {
    /// Leaf constants with fixed display strings.
    fn fmt_leaf_name(&self) -> Option<&str> {
        match self {
            Term::True => Some("true"),
            Term::False => Some("false"),
            Term::Unit => Some("()"),
            Term::Zero => Some("zero"),
            Term::Sorry => Some("sorry"),
            _ => None,
        }
    }

    /// Identify infix binary operator terms and return (lhs, rhs, operator string).
    fn fmt_infix_op(&self) -> Option<(&Term, &Term, &str)> {
        match self {
            Term::NatAdd(a, b) => Some((a, b, "+")),
            Term::NatSub(a, b) => Some((a, b, "-")),
            Term::NatMul(a, b) => Some((a, b, "*")),
            Term::NatDiv(a, b) => Some((a, b, "/")),
            Term::NatMod(a, b) => Some((a, b, "%")),
            Term::NatEq(a, b) => Some((a, b, "==")),
            Term::NatLt(a, b) => Some((a, b, "<")),
            Term::NatLe(a, b) => Some((a, b, "<=")),
            Term::NatGt(a, b) => Some((a, b, ">")),
            Term::NatGe(a, b) => Some((a, b, ">=")),
            Term::BoolAnd(a, b) => Some((a, b, "&&")),
            Term::BoolOr(a, b) => Some((a, b, "||")),
            _ => None,
        }
    }

    /// Identify keyword-binary terms with format `(keyword t1 t2)`.
    fn fmt_keyword_binary(&self) -> Option<(&str, &Term, &Term)> {
        match self {
            Term::StrConcat(a, b) => Some(("strconcat", a, b)),
            Term::StrEq(a, b) => Some(("streq", a, b)),
            Term::StrCharAt(a, b) => Some(("char_at", a, b)),
            Term::RefSet(a, b) => Some(("ref_set", a, b)),
            _ => None,
        }
    }

    /// Identify keyword-unary terms with format `(keyword t)`.
    fn fmt_keyword_unary(&self) -> Option<(&str, &Term)> {
        match self {
            Term::Succ(t) => Some(("succ", t)),
            Term::StrLen(t) => Some(("strlen", t)),
            Term::Fst(t) => Some(("fst", t)),
            Term::Snd(t) => Some(("snd", t)),
            Term::RefNew(t) => Some(("ref", t)),
            Term::RefGet(t) => Some(("ref_get", t)),
            Term::Return(t) => Some(("return", t)),
            _ => None,
        }
    }

    /// Identify typed-unary terms with format `(keyword [ty] t)`.
    fn fmt_typed_unary(&self) -> Option<(&str, &Type, &Term)> {
        match self {
            Term::Absurd(ty, t) => Some(("absurd", ty, t)),
            Term::Inl(ty, t) => Some(("inl", ty, t)),
            Term::Inr(ty, t) => Some(("inr", ty, t)),
            Term::Refl(ty, t) => Some(("refl", ty, t)),
            Term::Fold(ty, t) => Some(("fold", ty, t)),
            Term::Unfold(ty, t) => Some(("unfold", ty, t)),
            _ => None,
        }
    }
}
