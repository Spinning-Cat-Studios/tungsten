//! Owned `Term` → node-arena decomposition (ADR 2.7.26a §4, stage M).
//!
//! O(tree) nodes retained; used only at import boundaries (kernel results,
//! `Eq`-type term components, term-substitution results).

use super::TermNode;
use crate::ffi::types::nodes::import_type;
use crate::ffi::{Arena, TermHandle};
use crate::terms::Term;

/// Decompose an owned `Term` tree into arena nodes, returning the root
/// handle. Embedded `Type`s import into the type-node arena.
pub(crate) fn import_term(arena: &mut Arena, term: &Term) -> TermHandle {
    let node = match term {
        Term::True => TermNode::True,
        Term::False => TermNode::False,
        Term::Unit => TermNode::Unit,
        Term::Zero => TermNode::Zero,
        Term::Sorry => TermNode::Sorry,
        Term::NatLit(n) => TermNode::NatLit(*n),
        Term::Var(v) => TermNode::Var(v.clone()),
        Term::Global(g) => TermNode::Global(g.clone()),
        Term::StringLit(s) => TermNode::StringLit(s.clone()),
        Term::Lambda(v, ty, body) => {
            let ty = import_type(arena, ty);
            let body = import_term(arena, body);
            TermNode::Lambda(v.clone(), ty, body)
        }
        Term::App(a, b) => pair(arena, a, b, TermNode::App),
        Term::Let(v, ty, def, body) => {
            let ty = import_type(arena, ty);
            let def = import_term(arena, def);
            let body = import_term(arena, body);
            TermNode::Let(v.clone(), ty, def, body)
        }
        Term::If(c, t, e) => triple(arena, c, t, e, TermNode::If),
        Term::Absurd(ty, t) => typed_unary(arena, ty, t, TermNode::Absurd),
        Term::Succ(t) => TermNode::Succ(import_term(arena, t)),
        Term::NatRec(ty, a, b, c) => {
            let ty = import_type(arena, ty);
            let (a, b) = (import_term(arena, a), import_term(arena, b));
            let c = import_term(arena, c);
            TermNode::NatRec(ty, a, b, c)
        }
        Term::NatInd(ty, a, b, c) => {
            let ty = import_type(arena, ty);
            let (a, b) = (import_term(arena, a), import_term(arena, b));
            let c = import_term(arena, c);
            TermNode::NatInd(ty, a, b, c)
        }
        Term::NatAdd(a, b) => pair(arena, a, b, TermNode::NatAdd),
        Term::NatSub(a, b) => pair(arena, a, b, TermNode::NatSub),
        Term::NatMul(a, b) => pair(arena, a, b, TermNode::NatMul),
        Term::NatDiv(a, b) => pair(arena, a, b, TermNode::NatDiv),
        Term::NatMod(a, b) => pair(arena, a, b, TermNode::NatMod),
        Term::NatEq(a, b) => pair(arena, a, b, TermNode::NatEq),
        Term::NatLt(a, b) => pair(arena, a, b, TermNode::NatLt),
        Term::NatLe(a, b) => pair(arena, a, b, TermNode::NatLe),
        Term::NatGt(a, b) => pair(arena, a, b, TermNode::NatGt),
        Term::NatGe(a, b) => pair(arena, a, b, TermNode::NatGe),
        Term::BoolAnd(a, b) => pair(arena, a, b, TermNode::BoolAnd),
        Term::BoolOr(a, b) => pair(arena, a, b, TermNode::BoolOr),
        Term::BoolNot(t) => TermNode::BoolNot(import_term(arena, t)),
        Term::StrConcat(a, b) => pair(arena, a, b, TermNode::StrConcat),
        Term::StrLen(t) => TermNode::StrLen(import_term(arena, t)),
        Term::StrEq(a, b) => pair(arena, a, b, TermNode::StrEq),
        Term::StrCharAt(a, b) => pair(arena, a, b, TermNode::StrCharAt),
        Term::StrSubstring(a, b, c) => triple(arena, a, b, c, TermNode::StrSubstring),
        Term::Pair(a, b) => pair(arena, a, b, TermNode::Pair),
        Term::Fst(t) => TermNode::Fst(import_term(arena, t)),
        Term::Snd(t) => TermNode::Snd(import_term(arena, t)),
        Term::Inl(ty, t) => typed_unary(arena, ty, t, TermNode::Inl),
        Term::Inr(ty, t) => typed_unary(arena, ty, t, TermNode::Inr),
        Term::Case(s, v1, a, v2, b) => {
            let s = import_term(arena, s);
            let a = import_term(arena, a);
            let b = import_term(arena, b);
            TermNode::Case(s, v1.clone(), a, v2.clone(), b)
        }
        Term::TyAbs(v, t) => TermNode::TyAbs(v.clone(), import_term(arena, t)),
        Term::TyApp(t, ty) => {
            let t = import_term(arena, t);
            let ty = import_type(arena, ty);
            TermNode::TyApp(t, ty)
        }
        Term::Refl(ty, t) => typed_unary(arena, ty, t, TermNode::Refl),
        Term::Subst(ty, motive, a, b) => {
            let ty = import_type(arena, ty);
            let motive = import_type(arena, motive);
            let a = import_term(arena, a);
            let b = import_term(arena, b);
            TermNode::Subst(ty, motive, a, b)
        }
        Term::Fix(v, ty, body) => {
            let ty = import_type(arena, ty);
            let body = import_term(arena, body);
            TermNode::Fix(v.clone(), ty, body)
        }
        Term::Fold(ty, t) => typed_unary(arena, ty, t, TermNode::Fold),
        Term::Unfold(ty, t) => typed_unary(arena, ty, t, TermNode::Unfold),
        Term::ExternCall(name, args) => {
            let args = args.iter().map(|a| import_term(arena, a)).collect();
            TermNode::ExternCall(name.clone(), args)
        }
        Term::RefNew(t) => TermNode::RefNew(import_term(arena, t)),
        Term::RefGet(t) => TermNode::RefGet(import_term(arena, t)),
        Term::RefSet(a, b) => pair(arena, a, b, TermNode::RefSet),
        Term::Annot(t, ty) => {
            let t = import_term(arena, t);
            let ty = import_type(arena, ty);
            TermNode::Annot(t, ty)
        }
        Term::AdtConstruct(ty, idx, t) => {
            let ty = import_type(arena, ty);
            let t = import_term(arena, t);
            TermNode::AdtConstruct(ty, *idx, t)
        }
        Term::AdtMatch(s, arms) => {
            let s = import_term(arena, s);
            let arms = arms
                .iter()
                .map(|(idx, v, body)| (*idx, v.clone(), import_term(arena, body)))
                .collect();
            TermNode::AdtMatch(s, arms)
        }
        Term::Return(t) => TermNode::Return(import_term(arena, t)),
        Term::Spanned(t, span) => TermNode::Spanned(import_term(arena, t), *span),
        Term::IntLit(i) => TermNode::IntLit(*i),
        Term::IntBin(op, a, b) => {
            let (a, b) = (import_term(arena, a), import_term(arena, b));
            TermNode::IntBin(*op, a, b)
        }
        Term::IntNeg(t) => TermNode::IntNeg(import_term(arena, t)),
        Term::NatToInt(t) => TermNode::NatToInt(import_term(arena, t)),
        Term::IntToNat(t) => TermNode::IntToNat(import_term(arena, t)),
    };
    arena.alloc_term_node(node)
}

fn pair(
    arena: &mut Arena,
    a: &Term,
    b: &Term,
    make: fn(TermHandle, TermHandle) -> TermNode,
) -> TermNode {
    let (a, b) = (import_term(arena, a), import_term(arena, b));
    make(a, b)
}

fn triple(
    arena: &mut Arena,
    a: &Term,
    b: &Term,
    c: &Term,
    make: fn(TermHandle, TermHandle, TermHandle) -> TermNode,
) -> TermNode {
    let (a, b, c) = (
        import_term(arena, a),
        import_term(arena, b),
        import_term(arena, c),
    );
    make(a, b, c)
}

fn typed_unary(
    arena: &mut Arena,
    ty: &crate::types::Type,
    t: &Term,
    make: fn(crate::ffi::TypeHandle, TermHandle) -> TermNode,
) -> TermNode {
    let ty = import_type(arena, ty);
    let t = import_term(arena, t);
    make(ty, t)
}
