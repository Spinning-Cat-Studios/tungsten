//! Node-arena → owned `Term` reconstruction (ADR 2.7.26a §4, stage M).
//!
//! O(tree) transient — the result is freed by the caller (the bootstrap kernel
//! typechecker, the evaluator, diagnostics, term equality). `None` on an
//! invalid/dangling handle.

use super::TermNode;
use crate::ffi::types::nodes::materialize_type;
use crate::ffi::{Arena, TermHandle, TypeHandle};
use crate::terms::Term;

/// Rebuild an owned `Term` tree (with owned embedded `Type`s) from nodes.
pub(crate) fn materialize_term(arena: &Arena, handle: TermHandle) -> Option<Term> {
    let term = match arena.get_term_node(handle)? {
        TermNode::True => Term::True,
        TermNode::False => Term::False,
        TermNode::Unit => Term::Unit,
        TermNode::Zero => Term::Zero,
        TermNode::Sorry => Term::Sorry,
        TermNode::NatLit(n) => Term::NatLit(*n),
        TermNode::Var(v) => Term::Var(v.clone()),
        TermNode::Global(g) => Term::Global(g.clone()),
        TermNode::StringLit(s) => Term::StringLit(s.clone()),
        TermNode::Lambda(v, ty, body) => {
            Term::Lambda(v.clone(), mat_ty(arena, *ty)?, mat(arena, *body)?)
        }
        TermNode::App(a, b) => Term::App(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::Let(v, ty, def, body) => Term::Let(
            v.clone(),
            mat_ty(arena, *ty)?,
            mat(arena, *def)?,
            mat(arena, *body)?,
        ),
        TermNode::If(c, t, e) => Term::If(mat(arena, *c)?, mat(arena, *t)?, mat(arena, *e)?),
        TermNode::Absurd(ty, t) => Term::Absurd(mat_ty(arena, *ty)?, mat(arena, *t)?),
        TermNode::Succ(t) => Term::Succ(mat(arena, *t)?),
        TermNode::NatRec(ty, a, b, c) => Term::NatRec(
            mat_ty(arena, *ty)?,
            mat(arena, *a)?,
            mat(arena, *b)?,
            mat(arena, *c)?,
        ),
        TermNode::NatInd(ty, a, b, c) => Term::NatInd(
            mat_ty(arena, *ty)?,
            mat(arena, *a)?,
            mat(arena, *b)?,
            mat(arena, *c)?,
        ),
        TermNode::NatAdd(a, b) => Term::NatAdd(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::NatSub(a, b) => Term::NatSub(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::NatMul(a, b) => Term::NatMul(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::NatDiv(a, b) => Term::NatDiv(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::NatMod(a, b) => Term::NatMod(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::NatEq(a, b) => Term::NatEq(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::NatLt(a, b) => Term::NatLt(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::NatLe(a, b) => Term::NatLe(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::NatGt(a, b) => Term::NatGt(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::NatGe(a, b) => Term::NatGe(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::BoolAnd(a, b) => Term::BoolAnd(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::BoolOr(a, b) => Term::BoolOr(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::BoolNot(t) => Term::BoolNot(mat(arena, *t)?),
        TermNode::StrConcat(a, b) => Term::StrConcat(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::StrLen(t) => Term::StrLen(mat(arena, *t)?),
        TermNode::StrEq(a, b) => Term::StrEq(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::StrCharAt(a, b) => Term::StrCharAt(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::StrSubstring(a, b, c) => {
            Term::StrSubstring(mat(arena, *a)?, mat(arena, *b)?, mat(arena, *c)?)
        }
        TermNode::Pair(a, b) => Term::Pair(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::Fst(t) => Term::Fst(mat(arena, *t)?),
        TermNode::Snd(t) => Term::Snd(mat(arena, *t)?),
        TermNode::Inl(ty, t) => Term::Inl(mat_ty(arena, *ty)?, mat(arena, *t)?),
        TermNode::Inr(ty, t) => Term::Inr(mat_ty(arena, *ty)?, mat(arena, *t)?),
        TermNode::Case(s, v1, a, v2, b) => Term::Case(
            mat(arena, *s)?,
            v1.clone(),
            mat(arena, *a)?,
            v2.clone(),
            mat(arena, *b)?,
        ),
        TermNode::TyAbs(v, t) => Term::TyAbs(v.clone(), mat(arena, *t)?),
        TermNode::TyApp(t, ty) => Term::TyApp(mat(arena, *t)?, mat_ty(arena, *ty)?),
        TermNode::Refl(ty, t) => Term::Refl(mat_ty(arena, *ty)?, mat(arena, *t)?),
        TermNode::Subst(ty, motive, a, b) => Term::Subst(
            mat_ty(arena, *ty)?,
            mat_ty(arena, *motive)?,
            mat(arena, *a)?,
            mat(arena, *b)?,
        ),
        TermNode::Fix(v, ty, body) => Term::Fix(v.clone(), mat_ty(arena, *ty)?, mat(arena, *body)?),
        TermNode::Fold(ty, t) => Term::Fold(mat_ty(arena, *ty)?, mat(arena, *t)?),
        TermNode::Unfold(ty, t) => Term::Unfold(mat_ty(arena, *ty)?, mat(arena, *t)?),
        TermNode::ExternCall(name, args) => Term::ExternCall(
            name.clone(),
            args.iter()
                .map(|a| materialize_term(arena, *a))
                .collect::<Option<Vec<_>>>()?,
        ),
        TermNode::RefNew(t) => Term::RefNew(mat(arena, *t)?),
        TermNode::RefGet(t) => Term::RefGet(mat(arena, *t)?),
        TermNode::RefSet(a, b) => Term::RefSet(mat(arena, *a)?, mat(arena, *b)?),
        TermNode::Annot(t, ty) => Term::Annot(mat(arena, *t)?, mat_ty(arena, *ty)?),
        TermNode::AdtConstruct(ty, idx, t) => {
            Term::AdtConstruct(mat_ty(arena, *ty)?, *idx, mat(arena, *t)?)
        }
        TermNode::AdtMatch(s, arms) => Term::AdtMatch(
            mat(arena, *s)?,
            arms.iter()
                .map(|(idx, v, body)| mat(arena, *body).map(|b| (*idx, v.clone(), b)))
                .collect::<Option<Vec<_>>>()?,
        ),
        TermNode::Return(t) => Term::Return(mat(arena, *t)?),
        TermNode::Spanned(t, span) => Term::Spanned(mat(arena, *t)?, *span),
        TermNode::IntLit(i) => Term::IntLit(*i),
        TermNode::IntBin(op, a, b) => Term::IntBin(*op, mat(arena, *a)?, mat(arena, *b)?),
        TermNode::IntNeg(t) => Term::IntNeg(mat(arena, *t)?),
        TermNode::NatToInt(t) => Term::NatToInt(mat(arena, *t)?),
        TermNode::IntToNat(t) => Term::IntToNat(mat(arena, *t)?),
    };
    Some(term)
}

/// Boxed child materialization (every `Term` child is boxed).
fn mat(arena: &Arena, handle: TermHandle) -> Option<Box<Term>> {
    materialize_term(arena, handle).map(Box::new)
}

/// Embedded type materialization (owned, un-boxed positions use it too).
fn mat_ty(arena: &Arena, handle: TypeHandle) -> Option<crate::types::Type> {
    materialize_type(arena, handle)
}
