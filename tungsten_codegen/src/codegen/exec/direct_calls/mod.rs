//! Direct (uncurried) calling convention for known-arity functions.
//!
//! When a call site provides all arguments to a statically-known function,
//! we emit a single multi-argument call instead of a chain of closure
//! allocations and indirect calls (ADR 2.5.26b).
//!
//! For tail-position self-recursive direct calls, we emit `musttail` to
//! guarantee stack frame reuse.

pub(crate) mod decompose;
mod helpers;
mod musttail;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_classp;
#[cfg(test)]
mod tests_classp_abi;
#[cfg(test)]
mod tests_classp_edge;
#[cfg(test)]
mod tests_decompose;

use helpers::{collect_arrow_params, collect_saturated_generic_call, unwrap_lambda_chain};
pub(crate) use helpers::{direct_name, type_arity};

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::{BasicMetadataTypeEnum, BasicType};
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;
use tungsten_core::types::Type;

/// The resolved materials for one saturated direct call, threaded from
/// `try_compile_direct_call` into its musttail-dispatch helper as a unit
/// (avoids a 6-param helper — ADR 23.7.26c close-out).
struct DirectCallSite<'a, 'ctx> {
    /// The `$direct` entry-point symbol being called.
    direct: &'a str,
    /// The resolved `$direct` LLVM function.
    direct_fn: inkwell::values::FunctionValue<'ctx>,
    /// The base (extern/mono-instance) name behind `direct`.
    lookup_name: &'a str,
    /// Compiled args, slot 0 = null env ptr, 1.. = the call arguments.
    arg_vals: &'a [BasicValueEnum<'ctx>],
    /// `arg_vals` as call-site metadata values.
    args_meta: &'a [inkwell::values::BasicMetadataValueEnum<'ctx>],
}

impl<'ctx> CodeGen<'ctx> {
    /// Declare the direct entry point for a top-level function with arity > 1.
    ///
    /// Signature: `R @name$direct(ptr %env, A %a, B %b, C %c, ...)`
    /// where all parameters are flattened into a single LLVM function.
    pub(crate) fn declare_direct_entry(
        &mut self,
        name: &str,
        ty: &Type,
    ) -> Result<(), CodeGenError> {
        let arity = type_arity(ty);
        if arity <= 1 {
            return Ok(());
        }

        let (param_tys, ret_ty) = collect_arrow_params(ty);
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let ret_llvm = self.types.lower_type(ret_ty);

        let mut param_llvm: Vec<BasicMetadataTypeEnum<'ctx>> = vec![env_ptr_type.into()];
        for pt in &param_tys {
            param_llvm.push(self.types.lower_type(pt).into());
        }

        let fn_type = ret_llvm.fn_type(&param_llvm, false);
        let direct = direct_name(name);
        self.module.add_function(&direct, fn_type, None);
        self.direct_calls.set_arity(name, arity);

        Ok(())
    }

    /// Compile the direct entry point body for a top-level function.
    ///
    /// Unwraps the nested Lambda chain and compiles the innermost body
    /// with all parameters bound to the LLVM function arguments at once.
    pub(crate) fn compile_direct_entry(
        &mut self,
        name: &str,
        term: &tungsten_core::terms::Term,
        ty: &Type,
        span_start: Option<u32>,
    ) -> Result<(), CodeGenError> {
        let arity = match self.direct_calls.arity(name) {
            Some(a) => a,
            None => return Ok(()), // no direct entry for this function
        };

        let direct = direct_name(name);
        let function = self.module.get_function(&direct).ok_or_else(|| {
            CodeGenError::Unsupported(format!("direct entry '{direct}' not declared"))
        })?;

        self.compilation.current_fn = Some(function);
        self.direct_calls.current_entry = Some(direct.clone());

        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);
        self.compilation.env.clear();

        if let Some(span) = span_start {
            self.attach_debug_info_to_def(&direct, span, function);
        }

        // Unwrap Lambdas and bind each param to the corresponding LLVM arg.
        // LLVM arg 0 = env ptr (unused), args 1..arity = the actual params.
        let (param_names, body) = unwrap_lambda_chain(term, arity);
        let (param_tys_core, ret_ty) = collect_arrow_params(ty);

        for (i, (pname, pty)) in param_names.iter().zip(param_tys_core.iter()).enumerate() {
            let param_val = function.get_nth_param((i + 1) as u32).ok_or_else(|| {
                CodeGenError::TypeError(format!("direct entry '{direct}' missing param {i}"))
            })?;
            self.compilation
                .env
                .insert(pname.clone(), (param_val, (*pty).clone()));
        }

        // Body is in tail position
        self.compilation.in_tail_position = true;
        let result = self.compile_term(body)?;
        self.compilation.in_tail_position = false;

        let expected_ret_ty = self.types.lower_type(ret_ty);
        let result = self.cast_to_type(result, expected_ret_ty)?;
        self.emit_return_if_needed(&result)?;

        self.direct_calls.current_entry = None;
        self.verify_after_compile(&direct)?;

        Ok(())
    }

    /// Try to compile a saturated call as a direct call.
    ///
    /// Returns `Some(value)` if the call was lowered to a direct call,
    /// `None` if it should fall through to the closure path.
    pub(crate) fn try_compile_direct_call(
        &mut self,
        term: &tungsten_core::terms::Term,
        is_tail: bool,
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodeGenError> {
        let (callee_name, ty_args, arg_terms) = match collect_saturated_generic_call(term) {
            Some(triple) => triple,
            None => return Ok(None),
        };

        let lookup_name = match self.resolve_direct_call_target(&callee_name, &ty_args) {
            Some(name) => name,
            None => return Ok(None),
        };

        let arity = match self.direct_calls.arity(&lookup_name) {
            Some(a) => a,
            None => return Ok(None),
        };

        if arg_terms.len() != arity {
            return Ok(None); // not saturated
        }

        let direct = direct_name(&lookup_name);
        let direct_fn = match self.module.get_function(&direct) {
            Some(f) => f,
            None => return Ok(None),
        };

        // Compile all arguments (slot 0 = null env ptr).
        let mut arg_vals: Vec<BasicValueEnum<'ctx>> = Vec::with_capacity(arity + 1);
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        arg_vals.push(env_ptr_type.const_null().into());
        for arg_term in &arg_terms {
            let val = self.compile_term(arg_term)?;
            arg_vals.push(val);
        }

        let args_meta: Vec<inkwell::values::BasicMetadataValueEnum<'ctx>> =
            arg_vals.iter().map(|v| (*v).into()).collect();

        let site = DirectCallSite {
            direct: &direct,
            direct_fn,
            lookup_name: &lookup_name,
            arg_vals: &arg_vals,
            args_meta: &args_meta,
        };

        // Self-recursive tail call → musttail; otherwise fall through to a plain call.
        if let Some(result) = self.try_emit_saturated_musttail(&site, is_tail)? {
            return Ok(Some(result));
        }

        let call_site = self
            .builder
            .build_call(direct_fn, &args_meta, "direct_call")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        let result = call_site
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::TypeError("direct call returned void".to_string()))?;

        let result = self.materialize_call_result(result)?;
        Ok(Some(result))
    }

    /// Resolve the direct-call target symbol for a saturated call.
    ///
    /// A monomorphic call (`ty_args` empty) uses the extern-remapped global.
    /// A saturated *generic* call resolves its pre-registered mono-instance
    /// symbol (ADR 23.7.26c) so it rides `$direct` instead of the per-step-
    /// allocating curried wrapper chain; `None` (unregistered instance / a
    /// mono-blocking TyVar) falls the call site back to the closure path.
    fn resolve_direct_call_target(&self, callee_name: &str, ty_args: &[&Type]) -> Option<String> {
        if ty_args.is_empty() {
            Some(
                self.defs
                    .extern_name_map
                    .get(callee_name)
                    .cloned()
                    .unwrap_or_else(|| callee_name.to_string()),
            )
        } else {
            self.resolve_saturated_mono_callee(callee_name, ty_args)
        }
    }

    /// Emit a `musttail` self-recursive call when eligible, else `Ok(None)`.
    ///
    /// Handles the three tail-position cases: a direct self-call (`$direct` ==
    /// current entry), a decomposed self-call (inside a `$direct_mt` body), or
    /// a non-self-recursive/non-tail SKIP (traced). `Ok(None)` means the caller
    /// should emit a plain direct call.
    fn try_emit_saturated_musttail(
        &mut self,
        site: &DirectCallSite<'_, 'ctx>,
        is_tail: bool,
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodeGenError> {
        if !is_tail {
            self.trace_musttail(site.direct, "SKIP", "not in tail position");
            return Ok(None);
        }
        let current_direct = match self.direct_calls.current_entry.clone() {
            Some(entry) => entry,
            None => {
                self.trace_musttail(site.direct, "SKIP", "not in direct entry");
                return Ok(None);
            }
        };

        if current_direct == *site.direct {
            return self.try_emit_direct_musttail(site.direct_fn, site.args_meta);
        }

        // Inside a `$direct_mt` body: route a self-recursive call to the base
        // function through the decomposed path.
        if current_direct.ends_with(decompose::DIRECT_MT_SUFFIX) {
            let mt_base =
                &current_direct[..current_direct.len() - decompose::DIRECT_MT_SUFFIX.len()];
            if direct_name(mt_base) == *site.direct {
                // Skip slot 0 (env ptr): the decomposed path re-adds it.
                return self.try_emit_decomposed_musttail(site.lookup_name, &site.arg_vals[1..]);
            }
        }

        // A tail call to a different function. Recorded as its own decision
        // kind rather than left trace-only (ADR 5.8.26a D4) — see
        // `Decision::SkipNonSelf` for why it must not be folded into `Skip`.
        self.trace_musttail(site.direct, "SKIP", "not self-recursive");
        self.record_musttail_skip_non_self(site.direct, site.direct_fn.get_type());
        Ok(None)
    }

    // Musttail recording (`trace_musttail`, `record_musttail_*`) and the
    // `musttail call` emission primitives (`try_emit_direct_musttail`,
    // `emit_musttail_epilogue`) live in `musttail.rs`.
}
