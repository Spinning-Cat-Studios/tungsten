//! Shared `#[cfg(test)]` builders for the unfolding test modules.
//!
//! `termination.rs` and `diagnostic.rs` both need the ADR 11.8.26c reproducer
//! and the asymmetric mutual pair, and a fixture copied into two files is a
//! fixture that will disagree with itself. Lives beside them rather than inside
//! either, because neither owns it.

use tungsten_core::{Context, Type};

use crate::ast::Visibility;
use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
use crate::elaborate::Elaborator;
use crate::span::Span;

pub(super) fn type_def(name: &str, kind: TypeDefKind, encoding: Option<Type>) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        params: vec![],
        kind,
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: encoding,
        field_visibilities: Vec::new(),
    }
}

pub(super) fn ctor(name: &str, index: usize, fields: Vec<Type>) -> Constructor {
    Constructor {
        name: name.to_string(),
        fields,
        index,
        visibility: None,
        span: Span::new(0, 0),
    }
}

pub(super) fn make_elaborator() -> Elaborator<'static> {
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

/// The §1.1 reproducer, as the elaborator sees it.
///
/// `type Wrap<T> = W(T)` / `type Rose = Node(Wrap<Rose>)`. The encoding below
/// is not invented for the test: `tungsten info type type-encoding Rose` on the
/// real fixture prints `μα_Rose. α_Rose`, because the single-constructor
/// unwrapped strategy erases `Node`, and `Wrap` is parameterized so it has no
/// cached encoding of its own to expand into.
pub(super) fn elaborator_with_rose() -> Elaborator<'static> {
    let mut elab = make_elaborator();
    let rose_encoding = Type::mu("α_Rose", Type::TyVar("α_Rose".to_string()));

    elab.env.define_type(type_def(
        "Wrap",
        TypeDefKind::ADT(vec![ctor("W", 0, vec![Type::TyVar("T".to_string())])]),
        None,
    ));
    elab.env.define_type(type_def(
        "Rose",
        TypeDefKind::ADT(vec![ctor(
            "Node",
            0,
            vec![Type::App(
                "Wrap".to_string(),
                vec![Type::TyVar("Rose".to_string())],
            )],
        )]),
        Some(rose_encoding),
    ));
    elab
}

/// A mutual pair with **asymmetric** bodies: `A = AA | AB(B)`,
/// `B = BB(A, Nat)`.
///
/// Asymmetric deliberately. The pre-existing `A`/`B` fixture in `tests.rs`
/// gives both members the shape `Unit + …`, so a peel that resolved `α_B` to
/// the *wrong* member would produce a structurally identical answer and no
/// test could see it. Here `A` is a `Sum` and `B` is a `Product`, so the two
/// candidate resolutions are distinguishable — which is what makes
/// `unfold_mu_layers_disagrees_with_canonical_on_mutual_pair` meaningful.
pub(super) fn elaborator_with_asymmetric_mutual_pair() -> Elaborator<'static> {
    let mut elab = make_elaborator();

    // A = μα_A. μα_B. (Unit + α_B)
    let a_encoding = Type::mu(
        "α_A",
        Type::mu("α_B", Type::sum(Type::Unit, Type::TyVar("α_B".to_string()))),
    );
    // B = μα_A. μα_B. (α_A × Nat)
    let b_encoding = Type::mu(
        "α_A",
        Type::mu(
            "α_B",
            Type::product(Type::TyVar("α_A".to_string()), Type::Nat),
        ),
    );

    elab.env.define_type(type_def(
        "A",
        TypeDefKind::ADT(vec![
            ctor("AA", 0, vec![]),
            ctor("AB", 1, vec![Type::TyVar("B".to_string())]),
        ]),
        Some(a_encoding),
    ));
    elab.env.define_type(type_def(
        "B",
        TypeDefKind::ADT(vec![ctor(
            "BB",
            0,
            vec![Type::TyVar("A".to_string()), Type::Nat],
        )]),
        Some(b_encoding),
    ));
    elab
}
