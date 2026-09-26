//! `ProjectNormalizer` — a live whole-project normalization view (ADR 21.7.26j).
//!
//! Wraps an `Elaborator` seeded with every project type + constructor so the
//! multi-module path of `doctor check type determinism normalization` can
//! re-normalize `App(name, args)` against the *real* project type environment —
//! the §2.1 `encode(fresh) ≡ₙ stored` invariant — which standalone
//! re-elaboration cannot reach (it can't resolve cross-module imports).
//!
//! It lives in `elaborate` rather than `driver` because it needs the
//! `Elaborator`'s module-private `env`; the driver's
//! `elaborate_project_with_inspector` constructs it after Body Elaboration and
//! re-exports the type.

use std::collections::HashMap;

use tungsten_core::{Context, Type};

use super::{Elaborator, ModuleExports};

/// A normalization view over a whole-project type environment.
///
/// The seeding is raw `TypeDef` injection: `normalize_for_comparison`'s
/// `normalize_app`/`normalize_tyvar` re-derive each encoding from
/// `type_def.kind` + `params`, so no collection pass is required
/// (ADR 21.7.26j §3, seeding-fragility gate (c)).
///
/// Carries the stored Phase-1e encodings of the *same* elaboration so the
/// consistency check needs only one project elaboration (the map is a cheap
/// clone relative to elaboration itself), and compares stored-vs-fresh without
/// cross-run drift.
pub struct ProjectNormalizer<'a> {
    elaborator: Elaborator<'a>,
    stored_encodings: HashMap<String, Type>,
    /// Source-fresh Phase-1e encodings, re-derived per module by re-running the
    /// collection pass with the whole-project exports injected (ADR 22.7.26b).
    /// Unlike `normalize`, these fully expand records and generic
    /// instantiations, so they are a faithful comparand for the stored
    /// encodings — the oracle's primary comparison.
    per_module_fresh: HashMap<String, Type>,
}

impl<'a> ProjectNormalizer<'a> {
    /// Seed a fresh `Elaborator` (borrowing `ctx`) with the accumulated Phase-B
    /// exports: every module's real type + constructor definitions.
    /// `stored_encodings` are that elaboration's Phase-1e encodings;
    /// `per_module_fresh` are the per-module oracle's source-fresh re-derived
    /// encodings (ADR 22.7.26b — empty when the caller has no module tree,
    /// e.g. unit tests exercising only the normalize fallback).
    #[must_use]
    pub fn seeded(
        ctx: &'a mut Context,
        exports: &ModuleExports,
        stored_encodings: HashMap<String, Type>,
        per_module_fresh: HashMap<String, Type>,
    ) -> Self {
        let mut elaborator = Elaborator::new(ctx);
        for (name, def) in &exports.types {
            elaborator.env.types.insert(name.clone(), def.clone());
        }
        for (name, info) in &exports.constructors {
            elaborator
                .env
                .constructors
                .insert(name.clone(), info.clone());
        }
        ProjectNormalizer {
            elaborator,
            stored_encodings,
            per_module_fresh,
        }
    }

    /// The per-module oracle's source-fresh Phase-1e encoding for `name`, if
    /// its defining module's re-collection produced one (ADR 22.7.26b).
    #[must_use]
    pub fn per_module_fresh(&self, name: &str) -> Option<&Type> {
        self.per_module_fresh.get(name)
    }

    /// The stored Phase-1e encodings of the elaboration that built this view —
    /// the left-hand side of the `encode(fresh) ≡ₙ stored` comparison.
    #[must_use]
    pub fn stored_encodings(&self) -> &HashMap<String, Type> {
        &self.stored_encodings
    }

    /// Normalize `ty` for structural comparison against a stored encoding.
    ///
    /// The consistency check compares two normalized forms with `Type`'s
    /// derived structural `==` rather than `types_structurally_equal_normalized`
    /// deliberately: the latter's `types_structurally_equal_impl` has no `Adt`
    /// arm, so two identical `Type::Adt` values fall through to `false` — a
    /// latent gap this ADR's actioning surfaced (ADR 21.7.26j §2, would have
    /// flagged 44 healthy ADTs on `main.tg`). Derived `==` compares `Adt`
    /// correctly, and after normalization it is the `≡ₙ` relation the invariant
    /// asserts (normalization already uses exact μ-var names, so `==` loses no
    /// intended leniency).
    #[must_use]
    pub fn normalize(&self, ty: &Type) -> Type {
        self.elaborator.normalize_for_comparison(ty)
    }

    /// The declared type parameters of `name`, or `None` when the project's
    /// type environment has no such type — a stored encoding whose name is not
    /// reproducible from the whole-project export view (e.g. a dependency
    /// module's private type). Such names are reported as skips, not
    /// divergences (ADR 21.7.26j §3, follow-up gate (a)).
    #[must_use]
    pub fn type_params(&self, name: &str) -> Option<Vec<String>> {
        self.elaborator
            .env
            .lookup_type(name)
            .map(|td| td.params.clone())
    }
}
