//! Structural comparison intrinsics (ADR 29.6.26f, design doc §T11.2 / §T11.2a).
//!
//! - `__compare(a, b)` — concrete dispatch: infers `T`, emits `compare_T(a, b)`.
//! - `compare(a, b)` — generic dispatch: emits `App(App(TyApp(Global("__cmp"), T),
//!   a), b)` with abstract `T` allowed, resolved at instantiation.

use crate::ast::Expr;
use crate::span::Span;
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate `__compare(left, right)` — the **concrete** structural comparison
    /// intrinsic (ADR 29.6.26f §T11.2).
    ///
    /// Infers the operand type `T` and emits a call to `compare_T(left, right)`,
    /// whose body is *synthesized* as a `CoreDef` during compilation (the bootstrap
    /// authoring/synthesis split, design doc §T11.8). Rejects types this build
    /// cannot synthesize a comparator for.
    pub(in crate::elaborate::exprs) fn elab_compare(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        use crate::driver::output::format_type_for_display;

        if args.len() != 2 {
            return Err(ElabError::arity_mismatch(span, 2, args.len())
                .with_help("`__compare` takes exactly two arguments: __compare(left, right)"));
        }

        // Infer the left operand; the right must have the same type.
        let (left_term, left_ty_raw) = self.infer(&args[0])?;
        let right_term = self.check(&args[1], &left_ty_raw)?;

        // Strip the Phase-1c `@`-prefix (e.g. `TyVar("@Point")`) so the comparator
        // symbol and registered type are stable and match what synthesis sees.
        let left_ty = left_ty_raw.strip_tyvar_at_prefix();

        // Reject types this build cannot synthesize a comparator for. Records
        // let the check resolve named record types (`App`/`TyVar` of a record);
        // ADT definitions let it resolve a **generic instantiation** such as
        // `List<Nat>` (ADR 1.8.26c) — without them a direct `__compare` on one
        // would hard-error here while the gate accepts it, the
        // checker/synthesiser disagreement the gate exists to prevent.
        // μ-cluster members are not resolvable this early (the encodings are
        // not final until Encoding Finalization), so recursive clusters clear
        // this check and are decided at instantiation by the ADR 1.8.26b gate —
        // which is also why no `encoded_types`/provenance is passed: at this
        // point there are none to pass.
        let types = crate::comparator::ComparatorTypes::new(
            self.get_record_types(),
            &std::collections::HashMap::new(),
            &crate::elaborate::TypeProvenance::default(),
            self.get_adt_types(),
            &std::collections::HashMap::new(),
        );
        if !crate::comparator::synth::is_supported(&left_ty, &types) {
            return Err(ElabError::new(
                span,
                ElabErrorKind::ComparatorUnavailable(format_type_for_display(&left_ty)),
            ));
        }

        // Record the operand type so the synthesis pass can resolve the symbol
        // back to its `Type`, then emit `compare_T(left)(right)`. The global is
        // defined later by the comparator synthesis pass.
        let symbol = crate::comparator::mangling::comparator_symbol(&left_ty);
        crate::comparator::requests::register(symbol.clone(), left_ty.clone());
        let call = crate::comparator::terms::compare_app(&symbol, left_term, right_term);
        Ok((call, self.compare_result_type()))
    }

    /// Elaborate `compare(left, right)` — the **generic** structural comparison
    /// intrinsic (ADR 29.6.26f §T11.2a / P6′).
    ///
    /// Unlike `__compare`, the operand type `T` may be **abstract** (e.g. inside a
    /// generic `assert_eq<T>`): this emits `App(App(TyApp(Global("__cmp"), T), a), b)`,
    /// a deferred reference resolved at *instantiation* — by the monomorphizer
    /// (codegen) or by lazy synthesis (evaluator). There is **no** comparability
    /// gate here; rejection of incomparable types moves to instantiation (P3).
    pub(in crate::elaborate::exprs) fn elab_compare_poly(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        if args.len() != 2 {
            return Err(ElabError::arity_mismatch(span, 2, args.len())
                .with_help("`compare` takes exactly two arguments: compare(left, right)"));
        }

        let (left_term, left_ty_raw) = self.infer(&args[0])?;
        let right_term = self.check(&args[1], &left_ty_raw)?;
        // Strip the Phase-1c `@`-prefix so the type carried by the `TyApp` matches
        // what the instantiation-time resolver mangles.
        let operand_ty = left_ty_raw.strip_tyvar_at_prefix();

        // `compare<T>` — the comparator intrinsic; its instances are the
        // synthesized `compare_T` functions (resolved at instantiation).
        let cmp_fn = Term::TyApp(
            Box::new(Term::Global(
                crate::comparator::COMPARE_INTRINSIC.to_string(),
            )),
            operand_ty,
        );
        let call = Term::App(
            Box::new(Term::App(Box::new(cmp_fn), Box::new(left_term))),
            Box::new(right_term),
        );
        Ok((call, self.compare_result_type()))
    }

    /// The `CompareResult` type as seen by callers of the comparison intrinsics.
    ///
    /// Uses the cached encoding when available (so the inferred type unifies with
    /// an annotation's encoding); falls back to the unresolved named reference.
    fn compare_result_type(&self) -> Type {
        self.env
            .lookup_type("CompareResult")
            .and_then(|td| td.encoded_type.clone())
            .unwrap_or_else(|| Type::App("CompareResult".to_string(), vec![]))
    }
}
