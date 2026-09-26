//! Handle-children node representation for arena-stored terms
//! (ADR 2.7.26a §4, stage M).
//!
//! The owned-`Term` arena was the largest retention class of the self-compiled heap
//! wall (~40% of a 31 GiB RSS: 65k stored trees averaging ~203 KB — every
//! composite constructor deep-cloned its children AND embedded full owned
//! `Type` copies). A [`TermNode`] stores child *term handles* and embedded
//! *type handles*, so composing N nodes retains O(N) small nodes with all
//! structure shared. Owned `Term` trees exist only transiently, materialized
//! at consumption boundaries (kernel typecheck, the evaluator, diagnostics)
//! and freed by Rust afterwards.
//!
//! Mirrors every `Term` variant (including ones with no FFI constructor,
//! e.g. `AdtMatch`/`ExternCall`) so owned terms import losslessly.

pub(crate) mod import;
pub(crate) mod materialize;

pub(crate) use import::import_term;
pub(crate) use materialize::materialize_term;

use crate::ffi::{TermHandle, TypeHandle};
use crate::terms::{IntBinOp, TermSpan};

/// Arena node mirroring [`crate::terms::Term`], with children as handles.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TermNode {
    Var(String),
    Global(String),
    Lambda(String, TypeHandle, TermHandle),
    App(TermHandle, TermHandle),
    Let(String, TypeHandle, TermHandle, TermHandle),
    True,
    False,
    If(TermHandle, TermHandle, TermHandle),
    Unit,
    Absurd(TypeHandle, TermHandle),
    Zero,
    Succ(TermHandle),
    NatLit(u64),
    NatRec(TypeHandle, TermHandle, TermHandle, TermHandle),
    NatInd(TypeHandle, TermHandle, TermHandle, TermHandle),
    NatAdd(TermHandle, TermHandle),
    NatSub(TermHandle, TermHandle),
    NatMul(TermHandle, TermHandle),
    NatDiv(TermHandle, TermHandle),
    NatMod(TermHandle, TermHandle),
    NatEq(TermHandle, TermHandle),
    NatLt(TermHandle, TermHandle),
    NatLe(TermHandle, TermHandle),
    NatGt(TermHandle, TermHandle),
    NatGe(TermHandle, TermHandle),
    BoolAnd(TermHandle, TermHandle),
    BoolOr(TermHandle, TermHandle),
    BoolNot(TermHandle),
    StringLit(String),
    StrConcat(TermHandle, TermHandle),
    StrLen(TermHandle),
    StrEq(TermHandle, TermHandle),
    StrCharAt(TermHandle, TermHandle),
    StrSubstring(TermHandle, TermHandle, TermHandle),
    Pair(TermHandle, TermHandle),
    Fst(TermHandle),
    Snd(TermHandle),
    Inl(TypeHandle, TermHandle),
    Inr(TypeHandle, TermHandle),
    Case(TermHandle, String, TermHandle, String, TermHandle),
    TyAbs(String, TermHandle),
    TyApp(TermHandle, TypeHandle),
    Refl(TypeHandle, TermHandle),
    Subst(TypeHandle, TypeHandle, TermHandle, TermHandle),
    Fix(String, TypeHandle, TermHandle),
    Fold(TypeHandle, TermHandle),
    Unfold(TypeHandle, TermHandle),
    ExternCall(String, Vec<TermHandle>),
    RefNew(TermHandle),
    RefGet(TermHandle),
    RefSet(TermHandle, TermHandle),
    Annot(TermHandle, TypeHandle),
    Sorry,
    AdtConstruct(TypeHandle, usize, TermHandle),
    AdtMatch(TermHandle, Vec<(usize, String, TermHandle)>),
    Return(TermHandle),
    Spanned(TermHandle, TermSpan),
    IntLit(i64),
    IntBin(IntBinOp, TermHandle, TermHandle),
    IntNeg(TermHandle),
    NatToInt(TermHandle),
    IntToNat(TermHandle),
}

/// Heap bytes owned by ONE term node (strings + handle-vec slabs) — the
/// node arena's per-allocation retention contribution. Children are shared,
/// so there is deliberately no recursion (ADR 2.7.26a §3.4, node semantics).
pub(crate) fn node_heap_bytes_term(node: &TermNode) -> u64 {
    let handle_size = size_of::<TermHandle>() as u64;
    match node {
        TermNode::Var(s)
        | TermNode::Global(s)
        | TermNode::StringLit(s)
        | TermNode::Lambda(s, _, _)
        | TermNode::Let(s, _, _, _)
        | TermNode::TyAbs(s, _)
        | TermNode::Fix(s, _, _) => s.capacity() as u64,
        TermNode::Case(_, v1, _, v2, _) => (v1.capacity() + v2.capacity()) as u64,
        TermNode::ExternCall(name, args) => {
            name.capacity() as u64 + (args.capacity() as u64) * handle_size
        }
        TermNode::AdtMatch(_, arms) => {
            (arms.capacity() as u64) * size_of::<(usize, String, TermHandle)>() as u64
                + arms
                    .iter()
                    .map(|(_, v, _)| v.capacity() as u64)
                    .sum::<u64>()
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::Arena;
    use crate::terms::Term;
    use crate::types::Type;

    /// One sample of EVERY `Term` variant with non-trivial content, so the
    /// import/materialize round-trip exercises each conversion arm.
    fn sample_terms() -> Vec<Term> {
        let z = || Box::new(Term::Zero);
        let v = |s: &str| Box::new(Term::Var(s.into()));
        vec![
            Term::Var("x".into()),
            Term::Global("main".into()),
            Term::Lambda("x".into(), Type::Nat, v("x")),
            Term::App(v("f"), z()),
            Term::Let("y".into(), Type::Bool, Box::new(Term::True), v("y")),
            Term::True,
            Term::False,
            Term::If(Box::new(Term::True), z(), Box::new(Term::NatLit(1))),
            Term::Unit,
            Term::Absurd(Type::Void, v("bot")),
            Term::Zero,
            Term::Succ(z()),
            Term::NatLit(42),
            Term::NatRec(Type::Nat, z(), v("s"), z()),
            Term::NatInd(Type::Prop, z(), v("s"), z()),
            Term::NatAdd(z(), z()),
            Term::NatSub(z(), z()),
            Term::NatMul(z(), z()),
            Term::NatDiv(z(), z()),
            Term::NatMod(z(), z()),
            Term::NatEq(z(), z()),
            Term::NatLt(z(), z()),
            Term::NatLe(z(), z()),
            Term::NatGt(z(), z()),
            Term::NatGe(z(), z()),
            Term::IntLit(-7),
            Term::int_bin(IntBinOp::Sub, Term::IntLit(3), Term::IntLit(5)),
            Term::IntNeg(Box::new(Term::IntLit(1))),
            Term::NatToInt(z()),
            Term::IntToNat(Box::new(Term::IntLit(-1))),
            Term::BoolAnd(Box::new(Term::True), Box::new(Term::False)),
            Term::BoolOr(Box::new(Term::True), Box::new(Term::False)),
            Term::BoolNot(Box::new(Term::True)),
            Term::StringLit("hello".into()),
            Term::StrConcat(v("a"), v("b")),
            Term::StrLen(v("s")),
            Term::StrEq(v("a"), v("b")),
            Term::StrCharAt(v("s"), z()),
            Term::StrSubstring(v("s"), z(), z()),
            Term::Pair(z(), Box::new(Term::True)),
            Term::Fst(v("p")),
            Term::Snd(v("p")),
            Term::Inl(Type::sum(Type::Nat, Type::Bool), z()),
            Term::Inr(Type::sum(Type::Nat, Type::Bool), Box::new(Term::True)),
            Term::Case(v("s"), "l".into(), v("l"), "r".into(), v("r")),
            Term::TyAbs("a".into(), v("body")),
            Term::TyApp(v("f"), Type::Nat),
            Term::Refl(Type::Nat, z()),
            Term::Subst(Type::Nat, Type::Prop, v("eq"), v("w")),
            Term::Fix("f".into(), Type::arrow(Type::Nat, Type::Nat), v("f")),
            Term::Fold(
                Type::Mu(
                    "a".into(),
                    Box::new(Type::sum(Type::Unit, Type::TyVar("a".into()))),
                ),
                z(),
            ),
            Term::Unfold(
                Type::Mu(
                    "a".into(),
                    Box::new(Type::sum(Type::Unit, Type::TyVar("a".into()))),
                ),
                v("l"),
            ),
            Term::ExternCall("tg_print".into(), vec![Term::Zero, Term::True]),
            Term::RefNew(z()),
            Term::RefGet(v("r")),
            Term::RefSet(v("r"), z()),
            Term::Annot(z(), Type::Nat),
            Term::Sorry,
            Term::AdtConstruct(
                Type::adt("T", vec![], vec![("A".into(), Type::Unit)]),
                1,
                z(),
            ),
            Term::AdtMatch(v("s"), vec![(0, "x".into(), v("x")), (1, "y".into(), z())]),
            Term::early_return(Term::Zero),
            Term::Spanned(z(), TermSpan { start: 3, end: 9 }),
        ]
    }

    #[test]
    fn import_materialize_round_trips_every_variant() {
        let mut arena = Arena::new();
        for term in sample_terms() {
            let handle = import_term(&mut arena, &term);
            let back = materialize_term(&arena, handle).expect("valid handle");
            assert_eq!(back, term, "round-trip mismatch for {term:?}");
        }
    }

    #[test]
    fn composing_shares_children_instead_of_copying() {
        let mut arena = Arena::new();
        let leaf = import_term(&mut arena, &Term::Zero);
        let nodes_after_leaf = arena.terms.len();
        let mut current = leaf;
        for _ in 0..64 {
            current = arena.alloc_term_node(TermNode::App(current, current));
        }
        assert_eq!(arena.terms.len(), nodes_after_leaf + 64);
    }

    #[test]
    fn node_heap_bytes_counts_strings_and_slabs() {
        let mut name = String::from("case_l");
        name.shrink_to_fit();
        let cap = name.capacity() as u64;
        assert_eq!(
            node_heap_bytes_term(&TermNode::Case(0, name, 1, String::new(), 2)),
            cap
        );
        assert_eq!(node_heap_bytes_term(&TermNode::App(0, 1)), 0);
    }
}
