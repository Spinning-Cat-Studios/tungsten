//! Argument decomposition for musttail-eligible functions (ADR 18.5.26a).
//!
//! Emits `$direct_mt` (flattened scalar params + musttail) and a `$direct` shim
//! that unpacks struct fields and delegates, working around LLVM 18's crash on
//! struct params with musttail.
//!
//! Split across submodules by responsibility:
//! - [`layout`] — the `$direct_mt` parameter-slot layout (single source of truth).
//! - [`bind`] — callee-side param binding + sret return emission.
//! - [`shim`] — the `$direct` shim that recomposes params and delegates.
//! - [`recurse`] — the self-recursive `$direct_mt → $direct_mt` tail edge.

mod bind;
pub(crate) mod layout;
mod recurse;
mod shim;

pub(crate) use layout::{plan_mt_slots, MtSlotPlan, ParamLowering, ParamSlot};

use crate::codegen::abi::{LoweredSignature, LoweredSlot};
use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::BasicTypeEnum;
use inkwell::AddressSpace;

/// The `$direct_mt` suffix for musttail-decomposed entry points.
pub(crate) const DIRECT_MT_SUFFIX: &str = "$direct_mt";

/// Build the decomposed entry point name.
pub(crate) fn direct_mt_name(base: &str) -> String {
    format!("{base}{DIRECT_MT_SUFFIX}")
}

impl<'ctx> CodeGen<'ctx> {
    /// Classify each source parameter (env excluded) of a `$direct` function type
    /// into its `$direct_mt` [`ParamLowering`] (ADR 1.7.26e §2.2):
    /// - flattenable struct → [`ParamLowering::Decompose`] (18.5.26a),
    /// - non-flattenable struct → [`ParamLowering::Indirect`] (Class P, 1.7.26e),
    /// - anything else (scalar / `ptr` / recursive-ADT) → [`ParamLowering::Passthrough`].
    fn compute_param_lowerings(
        &self,
        fn_type: inkwell::types::FunctionType<'ctx>,
    ) -> Vec<ParamLowering> {
        fn_type
            .get_param_types()
            .iter()
            .skip(1) // env ptr
            .map(|param| {
                if Self::should_pass_param_by_indirect(*param) {
                    ParamLowering::Indirect
                } else if param.is_struct_type() {
                    // Flattenable struct (should_pass_param_by_indirect was false).
                    ParamLowering::Decompose(param.into_struct_type().count_fields())
                } else {
                    ParamLowering::Passthrough
                }
            })
            .collect()
    }

    /// Declare a decomposed `$direct_mt` entry point if the function is eligible.
    ///
    /// Returns `Some(lowerings)` if declared, `None` if not eligible (no struct
    /// params to flatten/indirect and a non-sret return → normal recursion).
    ///
    /// Emits `$direct_mt` whenever ANY of: a flattenable struct param
    /// (18.5.26a decompose), a non-flattenable struct param (1.7.26e indirect
    /// buffer), or a by-value aggregate return (1.7.26a sret). The signature uses
    /// the unified leading-`ptr`-run layout (§2.1): `[sret]? indirect… env after-env…`.
    pub(crate) fn declare_decomposed_entry(
        &mut self,
        name: &str,
        fn_type: inkwell::types::FunctionType<'ctx>,
    ) -> Result<Option<Vec<ParamLowering>>, CodeGenError> {
        let lowerings = self.compute_param_lowerings(fn_type);

        let ret_type = fn_type
            .get_return_type()
            .unwrap_or_else(|| self.context.bool_type().into());
        // ADR 1.7.26a: a by-value aggregate return lowers to a `void` function
        // whose first param is a result-out pointer.
        let sret = Self::should_return_by_sret(ret_type);

        let needs_mt = sret
            || lowerings
                .iter()
                .any(|l| !matches!(l, ParamLowering::Passthrough));
        if !needs_mt {
            return Ok(None); // no struct params, no sret → no $direct_mt
        }

        // R6: build the canonical lowered-signature descriptor ONCE; derive the
        // LLVM function type from it, attach its attributes to the declaration,
        // and store it — every call site and the musttail gate consume it.
        let sig = self.build_mt_lowered_signature(fn_type, &lowerings, sret, ret_type);
        let mt_fn_type = sig.fn_type(self.context);
        // Captured before the descriptor is handed to the store — this is the
        // ABI contract `info codegen indirect-abi` reports (ADR 17.7.26e).
        let slot_attrs = sig.describe_slots();
        let mt_name = direct_mt_name(name);
        let mt_fn = self.module.add_function(&mt_name, mt_fn_type, None);
        sig.attach_to_function(self.context, mt_fn);
        self.direct_calls.set_lowered_sig(name, sig);

        if lowerings
            .iter()
            .any(|l| matches!(l, ParamLowering::Decompose(_)))
        {
            self.trace_musttail_decompose_lowerings(name, fn_type, &lowerings);
        }
        // ADR 1.7.26b: the base `$direct` skips, but this `$direct_mt` musttails.
        self.record_musttail_decompose(&mt_name, mt_fn_type, &lowerings, sret, slot_attrs);

        Ok(Some(lowerings))
    }

    /// Build the canonical [`LoweredSignature`] for a `$direct_mt` entry in the
    /// unified slot order (`[sret]? indirect… env after-env…`, ADR 1.7.26e §2.1),
    /// with the §2.1 canonical slot attributes (`sret(%T) noalias nonnull align
    /// dereferenceable` on the sret slot; `noalias nonnull align
    /// dereferenceable` — no `byval` — on indirect-param slots, R6 as amended
    /// by ADR 17.7.26e).
    fn build_mt_lowered_signature(
        &mut self,
        fn_type: inkwell::types::FunctionType<'ctx>,
        lowerings: &[ParamLowering],
        sret: bool,
        ret_type: BasicTypeEnum<'ctx>,
    ) -> LoweredSignature<'ctx> {
        let ptr: BasicTypeEnum<'ctx> = self.context.ptr_type(AddressSpace::default()).into();
        let original = fn_type.get_param_types();
        let mut slots: Vec<LoweredSlot<'ctx>> = Vec::new();

        if sret {
            let (align, size) = (
                self.types.type_align(ret_type),
                self.types.type_size(ret_type),
            );
            slots.push(LoweredSlot::sret(ptr, ret_type, align, size));
        }
        // Indirect buffers (source order) lead, before env.
        for (lowering, param) in lowerings.iter().zip(original.iter().skip(1)) {
            if matches!(lowering, ParamLowering::Indirect) {
                let (align, size) = (self.types.type_align(*param), self.types.type_size(*param));
                slots.push(LoweredSlot::indirect_param(ptr, *param, align, size));
            }
        }
        slots.push(LoweredSlot::env(ptr));
        // After-env: decomposed scalars / passthrough values (source order).
        for (lowering, param) in lowerings.iter().zip(original.iter().skip(1)) {
            match lowering {
                ParamLowering::Decompose(field_count) => {
                    let st = param.into_struct_type();
                    for i in 0..*field_count {
                        if let Some(field) = st.get_field_type_at_index(i) {
                            slots.push(LoweredSlot::flat(field));
                        }
                    }
                }
                ParamLowering::Passthrough => slots.push(LoweredSlot::flat(*param)),
                ParamLowering::Indirect => {} // already placed before env
            }
        }

        LoweredSignature {
            call_conv: 0,
            is_var_args: false,
            ret: if sret { None } else { Some(ret_type) },
            slots,
        }
    }

    /// Compile the decomposed `$direct_mt` body: same as `$direct` but with
    /// struct params reconstructed from scalars in the environment, and
    /// self-recursive calls decomposed with musttail.
    ///
    /// Also rewrites the original `$direct` as a shim that unpacks and delegates.
    pub(crate) fn compile_decomposed_entry(
        &mut self,
        name: &str,
        term: &tungsten_core::terms::Term,
        ty: &tungsten_core::types::Type,
        span_start: Option<u32>,
        param_map: &[ParamLowering],
    ) -> Result<(), CodeGenError> {
        let arity = match self.direct_calls.arity(name) {
            Some(a) => a,
            None => return Ok(()),
        };

        let mt_name = direct_mt_name(name);
        let mt_fn = self.module.get_function(&mt_name).ok_or_else(|| {
            CodeGenError::Unsupported(format!("decomposed entry '{mt_name}' not declared"))
        })?;

        // ADR 1.7.26a: a `void`-returning `$direct_mt` uses the result-out-pointer
        // (sret-style) ABI; param 0 is the out-pointer and params shift by one.
        let sret = mt_fn.get_type().get_return_type().is_none();

        // ── Phase 1: Compile $direct_mt body ──
        self.compilation.current_fn = Some(mt_fn);
        self.direct_calls.current_entry = Some(mt_name.clone());

        let entry = self.context.append_basic_block(mt_fn, "entry");
        self.builder.position_at_end(entry);
        self.compilation.env.clear();

        if let Some(span) = span_start {
            self.attach_debug_info_to_def(&mt_name, span, mt_fn);
        }

        let (param_names, body) = super::helpers::unwrap_lambda_chain(term, arity);
        let (param_tys_core, _ret_ty) = super::helpers::collect_arrow_params(ty);

        self.bind_decomposed_params(mt_fn, &mt_name, &param_names, &param_tys_core, param_map)?;

        // Expose the sret pointee to `compile_return`: an early `return` in
        // this body must store through the out-pointer, not `ret void`
        // (ADR 3.7.26a).
        let expected_ret_ty = self
            .types
            .lower_type(super::helpers::collect_arrow_params(ty).1);
        self.compilation.current_sret_type = sret.then_some(expected_ret_ty);

        // Compile body in tail position
        self.compilation.in_tail_position = true;
        let result = self.compile_term(body)?;
        self.compilation.in_tail_position = false;
        self.compilation.current_sret_type = None;

        let result = self.cast_to_type(result, expected_ret_ty)?;
        if sret {
            self.emit_sret_return(mt_fn, &mt_name, &result)?;
        } else {
            self.emit_return_if_needed(&result)?;
        }

        self.direct_calls.current_entry = None;
        self.verify_after_compile(&mt_name)?;

        // ── Phase 2: Rewrite $direct as a shim ──
        self.compile_decompose_shim(name, &mt_name, mt_fn, param_map)?;

        Ok(())
    }

    /// Trace the flattenable-struct params being decomposed (cosmetic, gated on
    /// `--trace-musttail`). Bridges the [`ParamLowering`] plan to the existing
    /// `trace_musttail_decompose` renderer.
    fn trace_musttail_decompose_lowerings(
        &self,
        name: &str,
        fn_type: inkwell::types::FunctionType<'ctx>,
        lowerings: &[ParamLowering],
    ) {
        if !self.tracing.trace_musttail {
            return;
        }
        let original = fn_type.get_param_types();
        let decomposed: Vec<_> = lowerings
            .iter()
            .zip(original.iter().skip(1))
            .filter(|(l, _)| matches!(l, ParamLowering::Decompose(_)))
            .map(|(_, p)| *p)
            .collect();
        let flattened: Vec<_> = decomposed
            .iter()
            .filter(|p| p.is_struct_type())
            .flat_map(|p| {
                let st = p.into_struct_type();
                (0..st.count_fields()).filter_map(move |i| st.get_field_type_at_index(i))
            })
            .collect();
        self.trace_musttail_decompose(name, &decomposed, &flattened);
    }
}
