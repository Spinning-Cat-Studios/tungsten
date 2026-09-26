//! List-like `μ` spine term-builders (stack-safe, tail-recursive; ADR 29.6.26f P4).
//!
//! A cons-list `μα. NonRec + (Elem × α)` is compared by a **tail-recursive spine**
//! carrying a `Nat` index accumulator, so the "heads equal ⇒ recurse on tails" call
//! sits in tail position and is musttail-eliminated (via 1.7.26a's sret path) — O(1)
//! native stack for a spine of any length (the 100 000-element AC). Path segments are
//! **source-level** here: `[i]` (`Index`) for an element difference, `.len` (`Len`)
//! for a length mismatch — the §2.3 list grammar.

use tungsten_core::{Term, Type};

use super::{
    compare_app, compare_result_sum_ty, cons_path, equal_term, not_equal_path, path_seg,
    single_path,
};

/// `Index(n)` — a list/tuple element segment carrying a runtime `Nat`.
#[must_use]
pub fn seg_index(n: Term) -> Term {
    path_seg(2, n)
}

/// `Len` — a list length-mismatch segment.
#[must_use]
pub fn seg_len() -> Term {
    path_seg(4, Term::Unit)
}

/// Wrap a spine body in three operand lambdas `λl:τ. λr:τ. λi:Nat. <body>`.
#[must_use]
pub fn curry3(operand_ty: Type, body: Term) -> Term {
    Term::Lambda(
        "l".to_string(),
        operand_ty.clone(),
        Box::new(Term::Lambda(
            "r".to_string(),
            operand_ty,
            Box::new(Term::Lambda("i".to_string(), Type::Nat, Box::new(body))),
        )),
    )
}

/// Body of the list wrapper `compare_List_T`: seed the spine at index 0
/// (`spine(l)(r)(0)`). `spine_symbol` is the generated spine global.
#[must_use]
pub fn list_wrapper_body(spine_symbol: &str) -> Term {
    Term::App(
        Box::new(Term::App(
            Box::new(Term::App(
                Box::new(Term::Global(spine_symbol.to_string())),
                Box::new(Term::Var("l".to_string())),
            )),
            Box::new(Term::Var("r".to_string())),
        )),
        Box::new(Term::NatLit(0)),
    )
}

/// Body of the tail-recursive list spine `spine(l, r, i)` over
/// `list_ty = μα. NonRec + (Elem × α)`. `elem_symbol` compares the element type;
/// `spine_symbol` is this spine's own global (for the tail call).
///
/// ```text
/// case unfold l of
///   Inl _  => case unfold r of Inl _ => Equal | Inr _ => NotEqual[.len]
///   Inr lp => case unfold r of
///               Inl _  => NotEqual[.len]
///               Inr rp => case compare_Elem(fst lp, fst rp) of
///                           Equal      => spine(snd lp, snd rp, i+1)   // TAIL
///                           NotEqual d => NotEqual( Index(i) :: d.path )
/// ```
#[must_use]
pub fn list_spine_body(list_ty: &Type, elem_symbol: &str, spine_symbol: &str) -> Term {
    let var = |n: &str| Term::Var(n.to_string());
    let unfold = |v: &str| Term::Unfold(list_ty.clone(), Box::new(var(v)));
    let len_mismatch = || not_equal_path(single_path(seg_len()));

    // Inner: both operands are `Cons` (lp = (hl, tl), rp = (hr, tr)).
    let head_cmp = compare_app(
        elem_symbol,
        Term::Fst(Box::new(var("lp"))),
        Term::Fst(Box::new(var("rp"))),
    );
    let tail_recurse = Term::App(
        Box::new(Term::App(
            Box::new(Term::App(
                Box::new(Term::Global(spine_symbol.to_string())),
                Box::new(Term::Snd(Box::new(var("lp")))),
            )),
            Box::new(Term::Snd(Box::new(var("rp")))),
        )),
        Box::new(Term::Succ(Box::new(var("i")))),
    );
    // NotEqual( cons(Index(i), d.path), d.displays ) — attribute the diff to `[i]`.
    let indexed_diff = {
        let d = || var("d");
        let new_diff = Term::Pair(
            Box::new(cons_path(seg_index(var("i")), Term::Fst(Box::new(d())))),
            Box::new(Term::Snd(Box::new(d()))),
        );
        Term::Inr(compare_result_sum_ty(), Box::new(new_diff))
    };
    let both_cons = Term::Case(
        Box::new(head_cmp),
        "_eq".to_string(),
        Box::new(tail_recurse),
        "d".to_string(),
        Box::new(indexed_diff),
    );

    // `l = Cons`: check `r`.
    let l_cons = Term::Case(
        Box::new(unfold("r")),
        "_ru".to_string(),
        Box::new(len_mismatch()),
        "rp".to_string(),
        Box::new(both_cons),
    );
    // `l = Nil`: equal iff `r` is also Nil.
    let l_nil = Term::Case(
        Box::new(unfold("r")),
        "_ru".to_string(),
        Box::new(equal_term()),
        "_rp".to_string(),
        Box::new(len_mismatch()),
    );
    Term::Case(
        Box::new(unfold("l")),
        "_lu".to_string(),
        Box::new(l_nil),
        "lp".to_string(),
        Box::new(l_cons),
    )
}
