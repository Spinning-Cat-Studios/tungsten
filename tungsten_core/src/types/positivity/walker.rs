//! The mode-carrying occurrence walker (ADR 7.8.26e §2.1).
//!
//! **The invariant the whole rule rests on:** the walker *never* descends into
//! a referenced type's definition. It reports the reference and walks the
//! arguments at a mode derived from `param_occ`. Correctness therefore depends
//! entirely on the caller's group being the true SCC of the *complete* type
//! graph, and on `param_occ` being a fixpoint over that same graph.
//!
//! Every `Type` variant gets an explicit arm — there is no catch-all "assume
//! strictly positive" (D5).

use std::collections::HashSet;

use crate::terms::Term;
use crate::types::Type;

use super::defs::PositivityDefs;
use super::fixpoint::ParamOccs;
use super::lattice::{Mode, Occ};

/// One link in the chain of intermediate `(type, parameter)` pairs a violation
/// was inherited through. Empty for a direct arrow-domain occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ViaLink {
    /// The intermediate type whose parameter is not strictly positive.
    pub type_name: String,
    /// That type's parameter name.
    pub param: String,
}

/// What the walker reports. Implemented once for the parameter-strictness
/// fixpoint and once for the violation check, so both share one traversal.
///
/// Both methods default to ignoring the event: each consumer cares about
/// exactly one of the two, and an empty `fn` in the impl would be a mutation
/// site with no observable behaviour to assert.
pub(super) trait Observer {
    /// A reference to a named type (`TyVar` role (a), `App`, or `Adt`).
    fn named(&mut self, _name: &str, _mode: Mode, _via: &[ViaLink]) {}
    /// A reference to a parameter of the enclosing definition (`TyVar` role (c)).
    fn param(&mut self, _index: usize, _mode: Mode) {}
}

/// A single field walk. Construct one per constructor field: the memo and the
/// `via` stack are scoped to that walk, so a violation is always attributed to
/// the field it was found in.
pub(super) struct Walk<'a, O: Observer> {
    defs: &'a PositivityDefs,
    occs: &'a ParamOccs,
    /// Parameters of the *enclosing* definition, for `TyVar` role (c).
    params: &'a [String],
    /// `Forall`/`Mu` binders currently in scope, for `TyVar` role (b).
    bound: Vec<String>,
    /// Visited `(node address, mode)` pairs.
    ///
    /// Defensive rather than load-bearing on today's owned `Type` trees, where
    /// each node has exactly one path — but this repo has twice been bitten by
    /// exponential type-walk blowups (ADRs 2.7.26a, 3.7.26b), and the mode is
    /// one bit, so the memo is small and cannot lose a violation.
    memo: HashSet<(usize, Mode)>,
    via: Vec<ViaLink>,
    observer: &'a mut O,
}

impl<'a, O: Observer> Walk<'a, O> {
    pub(super) fn new(
        defs: &'a PositivityDefs,
        occs: &'a ParamOccs,
        params: &'a [String],
        observer: &'a mut O,
    ) -> Self {
        Walk {
            defs,
            occs,
            params,
            bound: Vec::new(),
            memo: HashSet::new(),
            via: Vec::new(),
            observer,
        }
    }

    /// Walk one constructor field, starting at [`Mode::Strict`].
    pub(super) fn field(&mut self, ty: &Type) {
        self.walk(ty, Mode::Strict);
    }

    fn walk(&mut self, ty: &Type, mode: Mode) {
        if !self.memo.insert((std::ptr::from_ref(ty) as usize, mode)) {
            return;
        }
        match ty {
            // Group members must not occur in a domain at *any* depth or
            // parity. There is no sign flip: `Forbidden` is absorbing.
            Type::Arrow(domain, codomain) => {
                self.walk(codomain, mode);
                self.walk(domain, Mode::Forbidden);
            }

            Type::Product(a, b) | Type::Sum(a, b) => {
                self.walk(a, mode);
                self.walk(b, mode);
            }

            Type::Forall(binder, body) | Type::Mu(binder, body) => {
                self.bound.push(binder.clone());
                self.walk(body, mode);
                self.bound.pop();
            }

            Type::TyVar(name) => self.tyvar(name, mode),

            Type::App(name, args) => self.named(name, args, None, mode),
            Type::Adt(name, args, variants) => self.named(name, args, Some(variants), mode),

            // A mutable cell is invariant in its contents.
            Type::Ref(inner) | Type::Ptr(inner) => self.walk(inner, Mode::Forbidden),

            // The witness terms carry embedded types too — walking only `ty`
            // would let a cycle hide inside a `refl`/`fold` annotation.
            Type::Eq(ty_arg, left, right) => {
                self.walk(ty_arg, Mode::Forbidden);
                self.walk_term(left);
                self.walk_term(right);
            }

            Type::Bool
            | Type::Nat
            | Type::Int
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String => {}

            // Poison: a positivity diagnostic on an already-failed type is the
            // cascade ADR 7.8.26d exists to prevent.
            Type::Error => {}
        }
    }

    /// `TyVar` has three distinct roles; resolve in order (b), (c), (a).
    ///
    /// Conflating them is the subtle-wrong-answer trap: `Display` strips the
    /// `@` prefix, so `TyVar("@List")` and `TyVar("List")` print identically
    /// while only one of them is a key any map is keyed by.
    fn tyvar(&mut self, raw: &str, mode: Mode) {
        let name = raw.strip_prefix('@').unwrap_or(raw);
        // (b) bound by an enclosing `Forall`/`Mu` — not an occurrence at all.
        if self.bound.iter().any(|b| b == raw || b == name) {
            return;
        }
        // (c) a parameter of the enclosing definition.
        if let Some(index) = self.params.iter().position(|p| p == name) {
            self.observer.param(index, mode);
            return;
        }
        // (a) a reference to another named type.
        self.observer.named(name, mode, &self.via);
    }

    /// An `App`/`Adt` head: report the occurrence, then dispatch each argument
    /// three-way on the head's computed parameter strictness (D2).
    fn named(
        &mut self,
        name: &str,
        args: &[Type],
        variants: Option<&[(String, Type)]>,
        mode: Mode,
    ) {
        self.observer.named(name, mode, &self.via);

        if let Some(def) = self.defs.get(name) {
            let param_occs = self.occs.get(name);
            for (index, arg) in args.iter().enumerate() {
                let occ = param_occs
                    .and_then(|occs| occs.get(index))
                    .copied()
                    .unwrap_or(Occ::Forbidden);
                let Some(arg_mode) = mode.descend(occ) else {
                    continue; // `Unused` — not walked at any mode.
                };
                let inherited = occ == Occ::Forbidden && mode == Mode::Strict;
                if inherited {
                    self.via.push(ViaLink {
                        type_name: name.to_string(),
                        param: def
                            .params
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| format!("#{index}")),
                    });
                }
                self.walk(arg, arg_mode);
                if inherited {
                    self.via.pop();
                }
            }
            // Variant payloads are NOT walked: `def` already covers them, and
            // descending into a referenced type's definition would break the
            // invariant this walker rests on.
            return;
        }

        // A lossy `TypeDefKind::Stub` lowers any complex `TypeExpr` to
        // `Type::Unit`, so escalating its arguments to `Forbidden` would report
        // on information that was discarded. Skipped, not doubted (D5).
        let arg_mode = if self.defs.is_stub(name) {
            mode
        } else {
            Mode::Forbidden
        };
        for arg in args {
            self.walk(arg, arg_mode);
        }
        // With no definition to consult, an inlined `Adt`'s payload list is the
        // only evidence there is. Payloads sit in the same position as the ADT.
        for (_, payload) in variants.unwrap_or(&[]) {
            self.walk(payload, mode);
        }
    }

    /// Walk every type embedded in an `Eq` witness term, at `Forbidden`.
    fn walk_term(&mut self, term: &Term) {
        term.for_each_embedded_type(|ty| self.walk(ty, Mode::Forbidden));
        term.for_each_subterm(|sub| self.walk_term(sub));
    }
}
