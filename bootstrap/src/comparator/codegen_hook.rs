//! Wiring the comparator-synthesis callback into codegen (ADR 29.6.26f P6′ step 2).
//!
//! The codegen backend (`tungsten_codegen`) resolves `TyApp(Global("__cmp"), T)` —
//! the generic `compare` intrinsic, once mono has made `T` concrete — by invoking a
//! callback that synthesizes `compare_T` and its transitive sub-comparators. As with
//! the evaluator (`super::eval`), `tungsten_codegen` cannot depend on `bootstrap`
//! (where synthesis lives), so the callback is supplied here and installed on the
//! `CodeGen`.

use std::rc::Rc;

use tungsten_codegen::ComparatorSynth;
use tungsten_core::Type;

use super::context::ComparatorTypes;
use super::{discover, mangling, synth};

/// Build the codegen comparator-synthesis callback over `types`.
///
/// Returns `Ok((top_symbol, defs))` for a concrete comparable `T` — where
/// `top_symbol` is the comparator to call and `defs` is the transitive closure of
/// `(name, term, type)` comparator definitions to emit — or `Err(path)` if `T` is
/// not comparable (the P3 rejection point on the codegen path), where `path`
/// locates the first incomparable field.
#[must_use]
pub fn codegen_synth(types: ComparatorTypes) -> ComparatorSynth {
    Rc::new(move |ty: &Type| {
        // Strip any residual Phase-1c `@`-prefix so record/ADT type args key into
        // `record_types` and mangle to the same symbol the closure emits. (The
        // dispatch site already strips `@`; this keeps the callback self-contained.)
        let ty = ty.strip_tyvar_at_prefix();
        // P3: reject with the path to the first incomparable field. An
        // unsettled expansion (ADR 1.8.26c) renders through the same `Err`,
        // saying so in words — this callback's error channel is a `String` the
        // backend prints, so the two classes are distinguished by wording here
        // rather than by variant as they are in `gate::classify`.
        synth::check_comparable(&ty, &types).map_err(|why| why.to_string())?;
        let closure = discover::synth_closure_for(&ty, &types);
        let top = mangling::comparator_symbol(&ty);
        let defs = closure
            .into_iter()
            .map(|d| (d.name, d.term.term, d.ty))
            .collect();
        Ok((top, defs))
    })
}
