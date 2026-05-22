//! Direct (uncurried) calling convention for known-arity functions.
//!
//! When a call site provides all arguments to a statically-known function,
//! we emit a single multi-argument call instead of a chain of closure
//! allocations and indirect calls (ADR 2.5.26b).
//!
//! For tail-position self-recursive direct calls, we emit `musttail` to
//! guarantee stack frame reuse.

mod decompose;
mod helpers;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_decompose;

use helpers::{collect_arrow_params, unwrap_lambda_chain};
pub(crate) use helpers::{collect_saturated_call, direct_name, type_arity};

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, LLVMTailCallKind};
use inkwell::AddressSpace;
use tungsten_core::types::Type;

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
        self.direct_calls.arities.insert(name.to_string(), arity);

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
        let arity = match self.direct_calls.arities.get(name) {
            Some(&a) => a,
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
        let (callee_name, arg_terms) = match collect_saturated_call(term) {
            Some(pair) => pair,
            None => return Ok(None),
        };

        // Check if callee was remapped (extern wrappers)
        let lookup_name = self
            .defs
            .extern_name_map
            .get(&callee_name)
            .cloned()
            .unwrap_or_else(|| callee_name.clone());

        let arity = match self.direct_calls.arities.get(&lookup_name) {
            Some(&a) => a,
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

        // Compile all arguments
        let mut arg_vals: Vec<BasicValueEnum<'ctx>> = Vec::with_capacity(arity + 1);
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        arg_vals.push(env_ptr_type.const_null().into()); // null env
        for arg_term in &arg_terms {
            let val = self.compile_term(arg_term)?;
            arg_vals.push(val);
        }

        let args_meta: Vec<inkwell::values::BasicMetadataValueEnum<'ctx>> =
            arg_vals.iter().map(|v| (*v).into()).collect();

        // musttail path: self-recursive direct call in tail position
        if is_tail {
            if let Some(ref current_direct) = self.direct_calls.current_entry.clone() {
                if *current_direct == direct {
                    if let Some(result) = self.try_emit_direct_musttail(direct_fn, &args_meta)? {
                        return Ok(Some(result));
                    }
                } else if current_direct.ends_with(decompose::DIRECT_MT_SUFFIX) {
                    // Inside $direct_mt: check if this is a self-recursive call
                    // to the base function and route through decomposed path.
                    let mt_base =
                        &current_direct[..current_direct.len() - decompose::DIRECT_MT_SUFFIX.len()];
                    let expected_direct = direct_name(mt_base);
                    if expected_direct == direct {
                        if let Some(result) = self.try_emit_decomposed_musttail(
                            &lookup_name,
                            &arg_vals[1..], // skip env ptr (added by decomposed path)
                        )? {
                            return Ok(Some(result));
                        }
                    } else {
                        self.trace_musttail(&direct, "SKIP", "not self-recursive");
                    }
                } else {
                    self.trace_musttail(&direct, "SKIP", "not self-recursive");
                }
            } else {
                self.trace_musttail(&direct, "SKIP", "not in direct entry");
            }
        } else {
            self.trace_musttail(&direct, "SKIP", "not in tail position");
        }

        // Normal direct call
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

    /// Emit `musttail call` + `ret` for a self-recursive direct call.
    ///
    /// Returns `Some(dummy)` if musttail was emitted, `None` if not eligible.
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
    fn try_emit_direct_musttail(
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

        // ABI safety: struct returns/params may be incompatible with
        // musttail depending on target and call kind (ADR 12.5.26e/f).
        // Direct self-recursive calls are safe on AArch64.
        if let Err(reason) = self.check_musttail_abi_safety(
            direct_fn.get_type(),
            crate::codegen::abi::MusttailCallKind::DirectSelfRecursive,
        ) {
            self.trace_musttail(fn_name, "SKIP", reason);
            return Ok(None);
        }

        self.trace_musttail(fn_name, "EMIT", "self-recursive, tail position");

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

    /// Emit a trace message for musttail decisions (when --trace-musttail is active).
    fn trace_musttail(&self, fn_name: &str, action: &str, reason: &str) {
        if self.tracing.trace_musttail {
            eprintln!("[musttail] {fn_name}: {action} ({reason})");
        }
    }

    /// Emit trace messages for decomposition decisions.
    pub(super) fn trace_musttail_decompose(
        &self,
        fn_name: &str,
        original_params: &[BasicTypeEnum<'ctx>],
        flattened: &[BasicTypeEnum<'ctx>],
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

    /// Emit a musttail call + ret + dead block. Shared by direct and decomposed paths.
    pub(super) fn emit_musttail_epilogue(
        &mut self,
        target_fn: inkwell::values::FunctionValue<'ctx>,
        args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
        label: &str,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let call = self
            .builder
            .build_call(target_fn, args, label)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        call.set_tail_call_kind(LLVMTailCallKind::LLVMTailCallKindMustTail);
        let result = call
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::TypeError(format!("{label} returned void")))?;
        self.builder
            .build_return(Some(&result))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
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
