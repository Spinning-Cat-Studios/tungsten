//! Lowering routes — the shared "which code path turned a Tungsten type into an
//! LLVM layout" core (ADR 12.7.26c P1/D4).
//!
//! `lower_type` dispatches on a type's *spelling*: a nullary ADT can arrive as
//! `TyVar(name)` (→ `lower_nullary_adt`), a parameterized one as
//! `Type::App(name, args)` (→ `lower_app`), the elaborator's structural
//! encoding as `Sum`/`Product`/`Mu` (→ `lower_type` proper), or the flat form
//! as `Type::Adt` (→ `lower_adt`). After 16d2f4f1 unified every path onto the
//! shared `tagged_union_blob_type` authority these routes *should* all produce
//! one layout — the split-brain where they didn't was the merge-arms miscompile.
//!
//! This module exposes one sealed entry point, [`TypeLowering::lower_via_route`],
//! so the merge-error diagnosis (P2), the `lowering-consistency` doctor check
//! (P3), and `info type lowering` (P4) all re-derive layouts through the *same*
//! core and therefore can never disagree about what a route produces.

use super::strip_named_prefix;
use super::TypeLowering;
use inkwell::types::BasicTypeEnum;
use tungsten_core::types::Type;

/// The closed set of spellings one Tungsten type can take at codegen (D4).
///
/// Anything new that turns a type into an LLVM layout must join this enum — and
/// thereby the consistency check — rather than becoming a silent fifth path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `TyVar(name)` → `lower_nullary_adt` (nullary ADTs only).
    Named,
    /// `Type::App(name, args)` → `lower_app`.
    App,
    /// The elaborator's structural encoding (`Sum`/`Product`/`Mu`/`Adt`) via
    /// `lower_type`.
    Structural,
    /// `Type::Adt(name, args, variants)` → `lower_adt`.
    FlatAdt,
}

impl Route {
    /// Every route, in the order the diagnostics report them.
    #[must_use]
    pub fn all() -> [Route; 4] {
        [Route::Named, Route::App, Route::Structural, Route::FlatAdt]
    }

    /// Human-facing label naming the route and the function it dispatches to —
    /// e.g. `"named-ADT route (lower_nullary_adt)"`, matching the merge-error
    /// wording in ADR 12.7.26c §2.1.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Route::Named => "named-ADT route (lower_nullary_adt)",
            Route::App => "app route (lower_app)",
            Route::Structural => "structural route (via lower_type)",
            Route::FlatAdt => "flat-ADT route (lower_adt)",
        }
    }

    /// Short tag for `--json` / tabular output (`"named"`, `"app"`, …).
    #[must_use]
    pub fn short(self) -> &'static str {
        match self {
            Route::Named => "named",
            Route::App => "app",
            Route::Structural => "structural",
            Route::FlatAdt => "flat-adt",
        }
    }
}

impl<'ctx> TypeLowering<'ctx> {
    /// Build the `Type` spelling a given route lowers, for the ADT `name`
    /// applied to `args`. Returns `None` when the route does not apply to this
    /// type (e.g. the `Named` route only spells nullary ADTs).
    fn route_spelling(&self, route: Route, name: &str, args: &[Type]) -> Option<Type> {
        let name = strip_named_prefix(name);
        // The canonical handle every route derives from: `App(name, args)`
        // covers both nullary (`args == []`) and parameterized ADTs, and
        // `expand_type` / `resolve_to_flat_adt` accept it uniformly.
        let app = Type::App(name.to_string(), args.to_vec());
        match route {
            // The named route is reachable only for a nullary ADT reference.
            Route::Named if args.is_empty() => Some(Type::TyVar(name.to_string())),
            Route::Named => None,
            Route::App => Some(app),
            Route::Structural => self.expand_type(&app),
            // The flat-adt spelling only applies where it actually occurs.
            Route::FlatAdt if self.flat_adt_route_applies(name) => self.resolve_to_flat_adt(&app),
            Route::FlatAdt => None,
        }
    }

    /// Does the flat-adt route (`Type::Adt` → `lower_adt`) apply to `name`?
    ///
    /// `Type::Adt` is only how the elaborator represents a *flat enum*: a
    /// non-recursive ADT with ≥2 constructors. Single-constructor ADTs lower to
    /// their bare payload, zero-constructor ADTs to an empty struct, and
    /// recursive ADTs to `ptr` — none is ever spelled `Type::Adt`, so
    /// `lower_adt` (which unconditionally builds the tagged blob) is never
    /// invoked for them. Forcing that route on those shapes via
    /// `resolve_to_flat_adt` would compare a spelling that never reaches
    /// codegen — a false divergence. `named`/`app`/`structural` all special-case
    /// those shapes identically and remain compared everywhere.
    fn flat_adt_route_applies(&self, name: &str) -> bool {
        let name = strip_named_prefix(name);
        match self.adt_types.get(name) {
            Some((_, ctors)) => ctors.len() >= 2 && !self.is_recursive_adt(name),
            None => false,
        }
    }

    /// Lower `name<args>` via a specific [`Route`], or `None` if the route does
    /// not apply. Each call recomputes from scratch: the by-name
    /// `adt_type_cache` is cleared for `name` first, so a later route cannot be
    /// handed an earlier route's cached layout — that shortcut would mask
    /// exactly the split-brain this core exists to detect.
    #[must_use]
    pub fn lower_via_route(
        &mut self,
        route: Route,
        name: &str,
        args: &[Type],
    ) -> Option<BasicTypeEnum<'ctx>> {
        let spelling = self.route_spelling(route, name, args)?;
        self.adt_type_cache.remove(strip_named_prefix(name));
        Some(self.lower_type(&spelling))
    }

    /// Lower `name<args>` via every applicable route, returning `(route, layout)`
    /// pairs. Shared by the consistency check and `info type lowering` (D5).
    #[must_use]
    pub fn route_layouts(
        &mut self,
        name: &str,
        args: &[Type],
    ) -> Vec<(Route, BasicTypeEnum<'ctx>)> {
        Route::all()
            .into_iter()
            .filter_map(|route| {
                self.lower_via_route(route, name, args)
                    .map(|ty| (route, ty))
            })
            .collect()
    }

    /// The head type name of a source `Type`, if it denotes a named ADT/record.
    /// `TyVar`/`App`/`Adt` carry a name; structural `Sum`/`Product`/`Mu` do not.
    fn type_head_name(ty: &Type) -> Option<(&str, &[Type])> {
        match ty {
            Type::TyVar(name) => Some((strip_named_prefix(name), &[])),
            Type::App(name, args) => Some((name.as_str(), args.as_slice())),
            Type::Adt(name, args, _) => Some((name.as_str(), args.as_slice())),
            _ => None,
        }
    }

    /// Fully expand a type to its structural form for denotational comparison,
    /// falling back to the type itself when it is already structural or unknown.
    fn expand_or_self(&self, ty: &Type) -> Type {
        self.expand_type(ty).unwrap_or_else(|| ty.clone())
    }

    /// Do two source types denote the *same* type, even under different
    /// spellings? `TyVar("Foo")` and `Foo`'s structural `Sum` encoding are the
    /// same denotation (the 16d2f4f1 case); this is the D2 denotational-sameness
    /// rule, not literal `Type` equality.
    #[must_use]
    pub fn same_denotation(&self, a: &Type, b: &Type) -> bool {
        if a == b {
            return true;
        }
        // Same head name ⇒ same type (both branches inferred to `CompareResult`).
        if let (Some((na, _)), Some((nb, _))) = (Self::type_head_name(a), Self::type_head_name(b)) {
            if na == nb {
                return true;
            }
        }
        // Otherwise compare structural expansions (`TyVar` vs its `Sum` form).
        self.expand_or_self(a) == self.expand_or_self(b)
    }

    /// Which route reproduces `observed` for this source type? Re-lowers via
    /// every applicable route of the type's head name and returns the first
    /// whose layout matches — `None` when the source type carries no head name,
    /// or no current route reproduces the layout (e.g. a since-removed route
    /// like the pre-16d2f4f1 W4 typed payload).
    #[must_use]
    pub fn attribute_route(
        &mut self,
        source_ty: &Type,
        observed: BasicTypeEnum<'ctx>,
    ) -> Option<Route> {
        let (name, args) = {
            let (n, a) = Self::type_head_name(source_ty)?;
            (n.to_string(), a.to_vec())
        };
        self.route_layouts(&name, &args)
            .into_iter()
            .find(|(_, layout)| *layout == observed)
            .map(|(route, _)| route)
    }

    /// Build the enriched merge-divergence diagnosis block (P2/D1/D2): names the
    /// shared Tungsten type when both arms denote it, and attributes each
    /// observed LLVM layout to the route that produced it. Returns `None` when
    /// neither arm carries a source type (nothing to enrich — the caller keeps
    /// today's message).
    #[must_use]
    pub fn diagnose_layout_divergence(
        &mut self,
        a_ty: Option<&Type>,
        a_layout: BasicTypeEnum<'ctx>,
        b_ty: Option<&Type>,
        b_layout: BasicTypeEnum<'ctx>,
    ) -> Option<String> {
        let (a_ty, b_ty) = (a_ty?, b_ty?);
        let same = self.same_denotation(a_ty, b_ty);
        let a_route = self.attribute_route(a_ty, a_layout);
        let b_route = self.attribute_route(b_ty, b_layout);

        let mut out = String::new();
        if same {
            let name = Self::type_head_name(a_ty)
                .map(|(n, _)| n.to_string())
                .unwrap_or_else(|| format!("{a_ty}"));
            out.push_str(&format!("  both arms are `{name}`\n"));
        } else {
            out.push_str(&format!("  arm 1 source type: `{a_ty}`\n"));
            out.push_str(&format!("  arm 2 source type: `{b_ty}`\n"));
        }
        out.push_str(&format!(
            "  arm 1 = {}   ← {}\n",
            a_layout.print_to_string(),
            route_note(a_route)
        ));
        out.push_str(&format!(
            "  arm 2 = {}   ← {}\n",
            b_layout.print_to_string(),
            route_note(b_route)
        ));
        if same {
            out.push_str(
                "  two routes lowered one type differently — run\n  \
                 `tungsten doctor check type lowering-consistency <file>`",
            );
        }
        Some(out)
    }
}

/// Render a route attribution, or a note that no current route reproduces the
/// layout (the pre-16d2f4f1 W4 typed-payload route is one such removed path).
fn route_note(route: Option<Route>) -> String {
    match route {
        Some(r) => r.label().to_string(),
        None => "no current route reproduces this layout".to_string(),
    }
}

// Tests: route_tests.rs
#[cfg(test)]
#[path = "route_tests.rs"]
mod route_tests;
