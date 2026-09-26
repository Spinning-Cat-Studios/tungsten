//! The self-recursive `$direct_mt → $direct_mt` tail edge (ADRs 18.5.26a,
//! 1.7.26a, 1.7.26e §2.5/§2.6). Builds the next-iteration argument list in the
//! unified slot order, forwards the incoming sret out-pointer and every incoming
//! indirect buffer pointer unchanged, stores each next aggregate into its
//! forwarded buffer in place, and emits `musttail … ; ret` — never bouncing
//! through `$direct`.
//!
//! §2.6 simultaneous-assignment safety is met by construction: `bind` eager-loads
//! each indirect param's *value* at entry, so every incoming `arg_val` here is an
//! SSA value independent of the buffers. Storing next values into the buffers
//! therefore cannot disturb any already-computed argument, regardless of
//! swap/duplicate/cross-param aliasing.

use super::{direct_mt_name, plan_mt_slots, ParamSlot};
use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};
use inkwell::AddressSpace;

impl<'ctx> CodeGen<'ctx> {
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

        let param_map = if let Some(m) = self.direct_calls.decompose_map(base_name) {
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

        // Verify we're actually in the $direct_mt function.
        if current_fn != mt_fn {
            self.trace_musttail(&mt_name, "SKIP", "not inside $direct_mt");
            return Ok(None);
        }

        // ABI safety (ADR 1.7.26e R6): gate on the stored canonical
        // LoweredSignature via the exact musttail-compatibility check. On the
        // self-edge caller and callee share the descriptor, so this enforces
        // the leading-`ptr`-run invariant (no residual by-value aggregate),
        // varargs-freedom, and attribute self-consistency.
        let sig = if let Some(s) = self.direct_calls.lowered_sig(base_name) {
            s.clone()
        } else {
            self.trace_musttail(&mt_name, "SKIP", "no lowered signature");
            return Ok(None);
        };
        if let Err(incompat) = sig.musttail_compatible(&sig) {
            self.trace_musttail(&mt_name, "SKIP", &incompat.describe());
            return Ok(None);
        }

        let mt_args = self.build_decomposed_musttail_args(mt_fn, &mt_name, &param_map, arg_vals)?;

        self.trace_musttail(&mt_name, "EMIT", "decomposed self-recursive, tail position");

        let dummy =
            self.emit_musttail_epilogue(mt_fn, &mt_args, "musttail_decomposed", Some(&sig))?;
        Ok(Some(dummy))
    }

    /// Build the slot-indexed next-iteration argument vector for the self-tail
    /// edge: forward the incoming sret out-ptr + env; for each indirect param
    /// store the next aggregate into the incoming buffer in place and forward the
    /// SAME pointer; decompose flattenable structs; pass others through.
    fn build_decomposed_musttail_args(
        &mut self,
        mt_fn: inkwell::values::FunctionValue<'ctx>,
        mt_name: &str,
        param_map: &[super::ParamLowering],
        arg_vals: &[BasicValueEnum<'ctx>],
    ) -> Result<Vec<BasicMetadataValueEnum<'ctx>>, CodeGenError> {
        let sret = mt_fn.get_type().get_return_type().is_none();
        let plan = plan_mt_slots(param_map, sret);
        let n_slots = mt_fn.count_params() as usize;
        let mut args: Vec<Option<BasicMetadataValueEnum<'ctx>>> = vec![None; n_slots];

        if sret {
            // Forward THIS $direct_mt's own out-pointer (param 0) unchanged so the
            // final base-case write lands in the original caller's buffer and the
            // self-call signature matches exactly (ADR 1.7.26a).
            let out_ptr = mt_fn.get_nth_param(0).ok_or_else(|| {
                CodeGenError::TypeError(format!("sret $direct_mt '{mt_name}' missing out-pointer"))
            })?;
            args[0] = Some(out_ptr.into());
        }
        let env_null = self.context.ptr_type(AddressSpace::default()).const_null();
        args[plan.env_index as usize] = Some(env_null.into());

        for (i, slot) in plan.slots.iter().enumerate() {
            let val = *arg_vals.get(i).ok_or_else(|| {
                CodeGenError::TypeError(format!("decomposed musttail '{mt_name}' missing arg {i}"))
            })?;
            match *slot {
                ParamSlot::Indirect { buf_index } => {
                    let buf = mt_fn.get_nth_param(buf_index).ok_or_else(|| {
                        CodeGenError::TypeError(format!(
                            "Class-P musttail '{mt_name}' missing indirect buffer {buf_index}"
                        ))
                    })?;
                    self.builder
                        .build_store(buf.into_pointer_value(), val)
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                    args[buf_index as usize] = Some(buf.into());
                }
                ParamSlot::Decompose { start, field_count } => {
                    let sv = val.into_struct_value();
                    for field_idx in 0..field_count {
                        let field = self
                            .builder
                            .build_extract_value(
                                sv,
                                field_idx,
                                &format!("mt_unpack.{i}.{field_idx}"),
                            )
                            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                        args[(start + field_idx) as usize] = Some(field.into());
                    }
                }
                ParamSlot::Passthrough { index } => {
                    args[index as usize] = Some(val.into());
                }
            }
        }

        args.into_iter()
            .enumerate()
            .map(|(idx, a)| {
                a.ok_or_else(|| {
                    CodeGenError::TypeError(format!(
                        "decomposed musttail '{mt_name}' slot {idx} unfilled"
                    ))
                })
            })
            .collect()
    }
}
