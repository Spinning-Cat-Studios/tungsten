//! Argument decomposition for musttail-eligible functions (ADR 18.5.26a).
//!
//! Emits `$direct_mt` (flattened scalar params + musttail) and a `$direct` shim
//! that unpacks struct fields and delegates, working around LLVM 18's crash on
//! struct params with musttail.

use crate::codegen::abi::MusttailCallKind;
use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::{BasicMetadataTypeEnum, BasicType};
use inkwell::values::{BasicValue, BasicValueEnum};
use inkwell::AddressSpace;

/// The `$direct_mt` suffix for musttail-decomposed entry points.
pub(crate) const DIRECT_MT_SUFFIX: &str = "$direct_mt";

/// Build the decomposed entry point name.
pub(crate) fn direct_mt_name(base: &str) -> String {
    format!("{base}{DIRECT_MT_SUFFIX}")
}

impl<'ctx> CodeGen<'ctx> {
    /// Declare a decomposed `$direct_mt` entry point if the function is eligible.
    ///
    /// Returns `Some(decompose_map)` if declared, `None` if not eligible.
    pub(crate) fn declare_decomposed_entry(
        &mut self,
        name: &str,
        fn_type: inkwell::types::FunctionType<'ctx>,
    ) -> Result<Option<Vec<Option<u32>>>, CodeGenError> {
        // Check if any struct params can be flattened
        let flattened = match self.check_decomposition_eligible(fn_type) {
            Some(f) => f,
            None => return Ok(None),
        };

        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let ret_type = fn_type
            .get_return_type()
            .unwrap_or_else(|| self.context.bool_type().into());

        // Build $direct_mt function type: env_ptr + flattened scalar params
        let mut mt_params: Vec<BasicMetadataTypeEnum<'ctx>> = vec![env_ptr_type.into()];
        for p in &flattened {
            mt_params.push((*p).into());
        }
        let mt_fn_type = ret_type.fn_type(&mt_params, false);

        let mt_name = direct_mt_name(name);
        self.module.add_function(&mt_name, mt_fn_type, None);

        // Build param_map: for each original param, None or Some(field_count)
        let original_params = fn_type.get_param_types();
        // Skip env ptr (param 0)
        let mut param_map = Vec::new();
        for param in original_params.iter().skip(1) {
            if param.is_struct_type() {
                let st = param.into_struct_type();
                param_map.push(Some(st.count_fields()));
            } else {
                param_map.push(None);
            }
        }

        self.trace_musttail_decompose(name, &original_params[1..], &flattened);

        Ok(Some(param_map))
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
        param_map: &[Option<u32>],
    ) -> Result<(), CodeGenError> {
        let arity = match self.direct_calls.arities.get(name) {
            Some(&a) => a,
            None => return Ok(()),
        };

        let mt_name = direct_mt_name(name);
        let mt_fn = self.module.get_function(&mt_name).ok_or_else(|| {
            CodeGenError::Unsupported(format!("decomposed entry '{mt_name}' not declared"))
        })?;

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

        // Compile body in tail position
        self.compilation.in_tail_position = true;
        let result = self.compile_term(body)?;
        self.compilation.in_tail_position = false;

        let expected_ret_ty = self
            .types
            .lower_type(super::helpers::collect_arrow_params(ty).1);
        let result = self.cast_to_type(result, expected_ret_ty)?;
        self.emit_return_if_needed(&result)?;

        self.direct_calls.current_entry = None;
        self.verify_after_compile(&mt_name)?;

        // ── Phase 2: Rewrite $direct as a shim ──
        self.compile_decompose_shim(name, &mt_name, mt_fn, param_map)?;

        Ok(())
    }

    /// Bind decomposed parameters: reconstruct structs from flattened scalars,
    /// pass scalars through directly, and insert all into the compilation env.
    fn bind_decomposed_params(
        &mut self,
        mt_fn: inkwell::values::FunctionValue<'ctx>,
        mt_name: &str,
        param_names: &[String],
        param_tys_core: &[&tungsten_core::types::Type],
        param_map: &[Option<u32>],
    ) -> Result<(), CodeGenError> {
        let mut mt_arg_idx: u32 = 1; // skip env ptr
        for (i, (pname, pty)) in param_names.iter().zip(param_tys_core.iter()).enumerate() {
            let param_val = if let Some(Some(field_count)) = param_map.get(i) {
                let (val, next_idx) = self.reconstruct_struct_from_scalars(
                    mt_fn,
                    pname,
                    pty,
                    *field_count,
                    mt_arg_idx,
                )?;
                mt_arg_idx = next_idx;
                val
            } else {
                let val = mt_fn.get_nth_param(mt_arg_idx).ok_or_else(|| {
                    CodeGenError::TypeError(format!(
                        "decomposed entry '{mt_name}' missing param {mt_arg_idx}"
                    ))
                })?;
                mt_arg_idx += 1;
                val
            };
            self.compilation
                .env
                .insert(pname.clone(), (param_val, (*pty).clone()));
        }
        Ok(())
    }

    /// Reconstruct a struct value from its flattened scalar components
    /// in the `$direct_mt` parameter list. Returns (value, `next_arg_idx`).
    fn reconstruct_struct_from_scalars(
        &mut self,
        mt_fn: inkwell::values::FunctionValue<'ctx>,
        pname: &str,
        pty: &tungsten_core::types::Type,
        field_count: u32,
        start_idx: u32,
    ) -> Result<(BasicValueEnum<'ctx>, u32), CodeGenError> {
        let llvm_ty = self.types.lower_type(pty);
        let struct_ty = llvm_ty.into_struct_type();
        let mut agg: BasicValueEnum<'ctx> = struct_ty.get_undef().into();
        let mut idx = start_idx;
        for field_idx in 0..field_count {
            let scalar = mt_fn.get_nth_param(idx).ok_or_else(|| {
                CodeGenError::TypeError(format!("reconstruct_struct missing param {idx}"))
            })?;
            agg = self
                .builder
                .build_insert_value(
                    agg.into_struct_value(),
                    scalar,
                    field_idx,
                    &format!("{pname}.repack.{field_idx}"),
                )
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                .as_basic_value_enum();
            idx += 1;
        }
        Ok((agg, idx))
    }

    /// Emit the `$direct` shim that unpacks struct params and calls `$direct_mt`.
    pub(crate) fn compile_decompose_shim(
        &mut self,
        name: &str,
        _mt_name: &str,
        mt_fn: inkwell::values::FunctionValue<'ctx>,
        param_map: &[Option<u32>],
    ) -> Result<(), CodeGenError> {
        let direct_name_str = super::helpers::direct_name(name);
        let direct_fn = self.module.get_function(&direct_name_str).ok_or_else(|| {
            CodeGenError::Unsupported(format!(
                "direct entry '{direct_name_str}' not declared for shim"
            ))
        })?;

        // Clear existing blocks from $direct (it was compiled by compile_direct_entry
        // before we got here, or we need to handle the case where it hasn't been compiled)
        // Since we call this INSTEAD of compile_direct_entry, $direct has been declared
        // but its body hasn't been compiled yet. Just add the entry block.
        let entry = self.context.append_basic_block(direct_fn, "entry");
        self.builder.position_at_end(entry);

        // Decompose struct params into scalars for the $direct_mt call
        let mut mt_args: Vec<inkwell::values::BasicMetadataValueEnum<'ctx>> = Vec::new();
        // Env ptr passthrough
        let env = direct_fn
            .get_nth_param(0)
            .ok_or_else(|| CodeGenError::TypeError("shim missing env ptr".to_string()))?;
        mt_args.push(env.into());

        for (orig_idx, map_entry) in (1u32..).zip(param_map) {
            let param = direct_fn.get_nth_param(orig_idx).ok_or_else(|| {
                CodeGenError::TypeError(format!(
                    "shim '{direct_name_str}' missing param {orig_idx}"
                ))
            })?;

            match map_entry {
                Some(field_count) => {
                    // Extract each field as a scalar
                    let sv = param.into_struct_value();
                    for field_idx in 0..*field_count {
                        let field = self
                            .builder
                            .build_extract_value(
                                sv,
                                field_idx,
                                &format!("unpack.{orig_idx}.{field_idx}"),
                            )
                            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                        mt_args.push(field.into());
                    }
                }
                None => {
                    mt_args.push(param.into());
                }
            }
        }

        let call = self
            .builder
            .build_call(mt_fn, &mt_args, "shim_call")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        let result = call
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::TypeError("shim call returned void".to_string()))?;

        self.builder
            .build_return(Some(&result))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        self.verify_after_compile(&direct_name_str)?;

        Ok(())
    }

    /// Emit a decomposed musttail call inside `$direct_mt`.
    pub(crate) fn try_emit_decomposed_musttail(
        &mut self,
        base_name: &str,
        arg_vals: &[BasicValueEnum<'ctx>],
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodeGenError> {
        let mt_name = direct_mt_name(base_name);
        let mt_fn = if let Some(f) = self.module.get_function(&mt_name) {
            f
        } else {
            self.trace_musttail(&mt_name, "SKIP", "no $direct_mt function");
            return Ok(None);
        };

        let param_map = if let Some(m) = self.direct_calls.decompose_maps.get(base_name) {
            m.clone()
        } else {
            self.trace_musttail(&mt_name, "SKIP", "no decompose map");
            return Ok(None);
        };

        let current_fn = if let Some(f) = self.compilation.current_fn {
            f
        } else {
            self.trace_musttail(&mt_name, "SKIP", "no current function");
            return Ok(None);
        };

        // Verify we're actually in the $direct_mt function
        if current_fn != mt_fn {
            self.trace_musttail(&mt_name, "SKIP", "not inside $direct_mt");
            return Ok(None);
        }

        // ABI safety: $direct_mt has flattened params, but struct return types
        // still crash LLVM 18. Check the mt function type.
        if let Err(reason) =
            self.check_musttail_abi_safety(mt_fn.get_type(), MusttailCallKind::DirectSelfRecursive)
        {
            self.trace_musttail(&mt_name, "SKIP", reason);
            return Ok(None);
        }

        // Build decomposed args: env ptr + flattened scalars
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let mut mt_args: Vec<inkwell::values::BasicMetadataValueEnum<'ctx>> =
            vec![env_ptr_type.const_null().into()];

        for (i, val) in arg_vals.iter().enumerate() {
            match param_map.get(i) {
                Some(Some(field_count)) => {
                    // Decompose struct into scalar fields
                    let sv = val.into_struct_value();
                    for field_idx in 0..*field_count {
                        let field = self
                            .builder
                            .build_extract_value(
                                sv,
                                field_idx,
                                &format!("mt_unpack.{i}.{field_idx}"),
                            )
                            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                        mt_args.push(field.into());
                    }
                }
                _ => {
                    mt_args.push((*val).into());
                }
            }
        }

        self.trace_musttail(&mt_name, "EMIT", "decomposed self-recursive, tail position");

        let dummy = self.emit_musttail_epilogue(mt_fn, &mt_args, "musttail_decomposed")?;
        Ok(Some(dummy))
    }
}
