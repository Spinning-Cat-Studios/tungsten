//! Musttail decision recording + low-level `musttail call` emission (ADR 1.7.26b).
//!
//! Two cohesive concerns kept out of the hot `direct_calls/mod.rs` control flow:
//! - **Recording** — `record_musttail_decision`/`_skip`/`_decompose` push a
//!   [`MusttailDecision`](crate::codegen::musttail_report::MusttailDecision) into
//!   the `CodeGen` report sink at the moment the ABI gate decides, and the
//!   `trace_*` helpers render `--trace-musttail` output.
//! - **Emission** — `try_emit_direct_musttail` (the by-value self-call, with the
//!   ABI-safety gate) and `emit_musttail_epilogue` (shared by the direct and
//!   decomposed paths) write the actual `musttail call` + `ret` + dead block.
//!   The call-site orchestration deciding *whether* to reach these lives in
//!   `mod.rs` (`try_emit_saturated_musttail`).

use super::decompose::ParamLowering;
use crate::codegen::backend::CodeGenError;
use crate::codegen::musttail_report::{Blocker, Decision, MusttailDecision};
use crate::codegen::CodeGen;
use inkwell::values::{BasicValueEnum, LLVMTailCallKind};

impl<'ctx> CodeGen<'ctx> {
    /// Record a structured musttail decision into the report sink.
    pub(super) fn record_musttail_decision(
        &mut self,
        fn_name: &str,
        fn_type: inkwell::types::FunctionType<'ctx>,
        decision: Decision,
        blockers: Vec<Blocker>,
    ) {
        let reasons = blockers.iter().map(|b| b.reason).collect();
        self.musttail_report.push(MusttailDecision {
            function: fn_name.to_string(),
            decision,
            reasons,
            blockers,
            lowered_sig: lowered_sig_string(fn_type),
            param_abi: Vec::new(),
            sret: false,
            slot_attrs: Vec::new(),
        });
    }

    /// Record a SKIP decision, computing its structured ABI blockers.
    pub(super) fn record_musttail_skip(
        &mut self,
        fn_name: &str,
        fn_type: inkwell::types::FunctionType<'ctx>,
    ) {
        let blockers = Self::compute_musttail_blockers(fn_type);
        self.record_musttail_decision(fn_name, fn_type, Decision::Skip, blockers);
    }

    /// Record a tail call to a *different* function (ADR 5.8.26a D4).
    ///
    /// Until this existed the branch was `trace_musttail` only, so a mutual-tail
    /// `f→g→f` cycle produced no record at all and the `tco-coverage --gate`
    /// iterated a row set that could not contain it — a gate that passes because
    /// its input is empty looks exactly like a gate that passes.
    ///
    /// The ABI blockers are computed the same way as for a self-recursive SKIP,
    /// because they are what makes the edge *interesting*: a Class-P callee
    /// (`NON_FLATTENABLE_PARAM`) is the shape mutual-tail `musttail` would have
    /// to handle, and recording it is what lets a future call-graph join tell a
    /// benign edge from a recursion-participating one.
    pub(super) fn record_musttail_skip_non_self(
        &mut self,
        fn_name: &str,
        fn_type: inkwell::types::FunctionType<'ctx>,
    ) {
        let blockers = Self::compute_musttail_blockers(fn_type);
        self.record_musttail_decision(fn_name, fn_type, Decision::SkipNonSelf, blockers);
    }

    /// Record a DECOMPOSE decision for a `$direct_mt` entry — the base `$direct`
    /// cannot musttail (flattenable struct param, or a non-flattenable one under
    /// ADR 1.7.26e) but the decomposed/indirect entry does, so the function
    /// achieves constant stack (ADRs 1.7.26b, 18.5.26a, 1.7.26e). Carries the
    /// per-source-param indirect-ABI lowering for `info codegen indirect-abi`.
    pub(super) fn record_musttail_decompose(
        &mut self,
        mt_name: &str,
        mt_fn_type: inkwell::types::FunctionType<'ctx>,
        lowerings: &[ParamLowering],
        sret: bool,
        slot_attrs: Vec<String>,
    ) {
        let param_abi = lowerings.iter().map(|l| l.to_abi_kind()).collect();
        self.musttail_report.push(MusttailDecision {
            function: mt_name.to_string(),
            decision: Decision::Decompose,
            reasons: Vec::new(),
            blockers: Vec::new(),
            lowered_sig: lowered_sig_string(mt_fn_type),
            param_abi,
            sret,
            slot_attrs,
        });
    }
}

/// Render a lowered function type as a compact signature display, e.g.
/// `{ i32, [56 x i8] }(ptr, {…}, ptr)` (ADR 1.7.26b §2.4).
fn lowered_sig_string(fn_type: inkwell::types::FunctionType<'_>) -> String {
    let ret = fn_type
        .get_return_type()
        .map_or_else(|| "void".to_string(), |r| r.print_to_string().to_string());
    let params: Vec<String> = fn_type
        .get_param_types()
        .iter()
        .map(|p| p.print_to_string().to_string())
        .collect();
    format!("{ret}({})", params.join(", "))
}

impl<'ctx> CodeGen<'ctx> {
    /// Emit a trace message for musttail decisions (when --trace-musttail is active).
    pub(super) fn trace_musttail(&self, fn_name: &str, action: &str, reason: &str) {
        if self.tracing.trace_musttail {
            eprintln!("[musttail] {fn_name}: {action} ({reason})");
        }
    }

    /// Emit trace messages for decomposition decisions.
    pub(super) fn trace_musttail_decompose(
        &self,
        fn_name: &str,
        original_params: &[inkwell::types::BasicTypeEnum<'ctx>],
        flattened: &[inkwell::types::BasicTypeEnum<'ctx>],
    ) {
        if !self.tracing.trace_musttail {
            return;
        }
        let descs: Vec<String> = original_params
            .iter()
            .filter(|p| p.is_struct_type())
            .map(|p| {
                let st = p.into_struct_type();
                let fields: Vec<String> = (0..st.count_fields())
                    .filter_map(|i| st.get_field_type_at_index(i))
                    .map(|f| format!("{f:?}"))
                    .collect();
                format!("{{ {} }}", fields.join(", "))
            })
            .collect();
        eprintln!(
            "[musttail] {}$direct: DECOMPOSE ({} → {} scalar args)",
            fn_name,
            descs.join(", "),
            flattened.len(),
        );
    }
}

impl<'ctx> CodeGen<'ctx> {
    /// Emit `musttail call` + `ret` for a self-recursive direct call.
    ///
    /// Returns `Some(dummy)` if musttail was emitted, `None` if not eligible.
    ///
    /// # Dummy contract (ADR 2.7.26b T3)
    ///
    /// The returned `Some(dummy)` is a **typed placeholder in an unreachable
    /// dead block** (see [`Self::emit_musttail_epilogue`]) — it is NOT the
    /// call's result and its type is arbitrary (`i1 false` for a void/sret
    /// callee). Merge lowerings MUST exclude it from result-type unification:
    /// check the arm's end-block reachability and drop terminated arms from
    /// the phi entirely (the shared merge planner, ADR 2.7.26b §2.2). Typing a
    /// phi from this dummy was the 1.7.26e §6.6 miscompile.
    ///
    /// For self-recursive calls, the caller and callee are the same LLVM function,
    /// so their signatures are guaranteed identical. However, LLVM's `AArch64`
    /// backend does not support `musttail` with `sret` (indirect return via
    /// pointer for structs > 16 bytes). We guard on return type size.
    /// See ADR 8.5.26c for rationale.
    ///
    /// # LLVM verifier vs backend distinction
    ///
    /// The LLVM IR verifier accepts `musttail` as long as caller and callee
    /// signatures match. However, the backend (`SelectionDAGISel` on `AArch64`)
    /// can still reject the lowered `musttail` with a fatal `report_fatal_error`
    /// abort — this is NOT a verifier diagnostic but an unrecoverable crash.
    /// The ABI guards in `check_musttail_abi_safety` exist to prevent this
    /// backend-level failure.
    pub(super) fn try_emit_direct_musttail(
        &mut self,
        direct_fn: inkwell::values::FunctionValue<'ctx>,
        args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodeGenError> {
        let fn_name = direct_fn.get_name().to_str().unwrap_or("<unknown>");

        let current_fn = if let Some(f) = self.compilation.current_fn {
            f
        } else {
            self.trace_musttail(fn_name, "SKIP", "no current function");
            return Ok(None);
        };

        // musttail requires identical function types
        if direct_fn.get_type() != current_fn.get_type() {
            self.trace_musttail(fn_name, "SKIP", "function type mismatch");
            return Ok(None);
        }

        // ABI safety (ADR 1.7.26e R6): exact lowered-signature gate. Both sides
        // are plain by-value entries (no indirect lowering — that path lives in
        // `decompose/recurse.rs`), so describe each with its flat descriptor and
        // compare: a by-value struct return/param, varargs, or any signature
        // mismatch is a blocker (LLVM 18 rejects musttail + by-value struct).
        let caller_sig =
            crate::codegen::abi::LoweredSignature::from_flat_fn_type(current_fn.get_type());
        let callee_sig =
            crate::codegen::abi::LoweredSignature::from_flat_fn_type(direct_fn.get_type());
        if let Err(incompat) = caller_sig.musttail_compatible(&callee_sig) {
            self.trace_musttail(fn_name, "SKIP", &incompat.describe());
            self.record_musttail_skip(fn_name, direct_fn.get_type());
            return Ok(None);
        }

        self.trace_musttail(fn_name, "EMIT", "self-recursive, tail position");
        self.record_musttail_decision(fn_name, direct_fn.get_type(), Decision::Emit, Vec::new());

        let call_site = self
            .builder
            .build_call(direct_fn, args, "musttail_direct")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        call_site.set_tail_call_kind(LLVMTailCallKind::LLVMTailCallKindMustTail);

        let result = call_site.try_as_basic_value().left().ok_or_else(|| {
            CodeGenError::TypeError("musttail direct call returned void".to_string())
        })?;

        // musttail must be immediately followed by ret
        self.builder
            .build_return(Some(&result))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Dead block for subsequent code
        if let Some(function) = self.compilation.current_fn {
            let dead_bb = self
                .context
                .append_basic_block(function, "musttail_direct_dead");
            self.builder.position_at_end(dead_bb);
        }

        let dummy = direct_fn.get_type().get_return_type().map_or_else(
            || self.context.bool_type().const_zero().into(),
            inkwell::types::BasicTypeEnum::const_zero,
        );
        Ok(Some(dummy))
    }

    /// Emit a musttail call + ret + dead block. Shared by direct and decomposed paths.
    ///
    /// `sig` — the callee's canonical [`LoweredSignature`] when it has one
    /// (Class-P / decomposed entries): its slot attributes are attached to the
    /// call site so caller and callee agree exactly (R6, `musttail` demands it).
    ///
    /// # Dummy contract (ADR 2.7.26b T3)
    ///
    /// The returned value is a **typed placeholder emitted into the fresh dead
    /// block** this function leaves the builder positioned in — the block has
    /// no predecessors and control never reaches it (the real control flow
    /// ended at the `musttail call; ret`). Consumers (merge lowerings) MUST
    /// NOT let this value drive type decisions: check arm reachability before
    /// result-type unification and exclude terminated arms from the phi
    /// entirely — see the shared merge planner (ADR 2.7.26b §2.2) and the
    /// 1.7.26e §6.6 miscompile it prevents.
    pub(super) fn emit_musttail_epilogue(
        &mut self,
        target_fn: inkwell::values::FunctionValue<'ctx>,
        args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
        label: &str,
        sig: Option<&crate::codegen::abi::LoweredSignature<'ctx>>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let call = self
            .builder
            .build_call(target_fn, args, label)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        call.set_tail_call_kind(LLVMTailCallKind::LLVMTailCallKindMustTail);
        if let Some(sig) = sig {
            sig.attach_to_call_site(self.context, call);
        }
        // ADR 1.7.26a: an sret-style `$direct_mt` returns void — the result is
        // written through the out-pointer, so `musttail` is followed by `ret void`.
        if target_fn.get_type().get_return_type().is_none() {
            self.builder
                .build_return(None)
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        } else {
            let result = call
                .try_as_basic_value()
                .left()
                .ok_or_else(|| CodeGenError::TypeError(format!("{label} returned void")))?;
            self.builder
                .build_return(Some(&result))
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }
        if let Some(function) = self.compilation.current_fn {
            let dead = self
                .context
                .append_basic_block(function, &format!("{label}_dead"));
            self.builder.position_at_end(dead);
        }
        let dummy = target_fn.get_type().get_return_type().map_or_else(
            || self.context.bool_type().const_zero().into(),
            inkwell::types::BasicTypeEnum::const_zero,
        );
        Ok(dummy)
    }
}
