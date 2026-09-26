//! Callee-side parameter binding and sret return emission for the decomposed
//! `$direct_mt` entry (ADRs 18.5.26a, 1.7.26a, 1.7.26e).

use super::{plan_mt_slots, ParamLowering, ParamSlot};
use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicValue, BasicValueEnum};

impl<'ctx> CodeGen<'ctx> {
    /// Bind `$direct_mt` parameters into the compilation env, per the unified
    /// slot plan (ADR 1.7.26e §2.1):
    /// - **Indirect** (Class-P non-flattenable struct): **eager-load** the
    ///   aggregate *value* from its caller-owned buffer `ptr` once at entry, then
    ///   bind the value. Because every body read then uses this SSA value (never
    ///   the buffer), the §2.6 read-before-overwrite obligation is satisfied by
    ///   construction — the tail edge may store the next value into the buffer
    ///   without disturbing any read.
    /// - **Decompose** (flattenable struct): reconstruct from scalar fields (18.5.26a).
    /// - **Passthrough**: bind the arg directly.
    ///
    /// The sret out-pointer (if any) and indirect buffers lead the signature
    /// before `env`; decomposed/passthrough args follow `env` (see [`plan_mt_slots`]).
    pub(crate) fn bind_decomposed_params(
        &mut self,
        mt_fn: inkwell::values::FunctionValue<'ctx>,
        mt_name: &str,
        param_names: &[String],
        param_tys_core: &[&tungsten_core::types::Type],
        param_map: &[ParamLowering],
    ) -> Result<(), CodeGenError> {
        let sret = mt_fn.get_type().get_return_type().is_none();
        let plan = plan_mt_slots(param_map, sret);
        for (i, (pname, pty)) in param_names.iter().zip(param_tys_core.iter()).enumerate() {
            let slot = plan.slots.get(i).copied().ok_or_else(|| {
                CodeGenError::TypeError(format!("decomposed entry '{mt_name}' missing slot {i}"))
            })?;
            let param_val = match slot {
                ParamSlot::Indirect { buf_index } => {
                    self.load_indirect_param(mt_fn, mt_name, pname, pty, buf_index)?
                }
                ParamSlot::Decompose { start, field_count } => {
                    let (val, _next) = self.reconstruct_struct_from_scalars(
                        mt_fn,
                        pname,
                        pty,
                        field_count,
                        start,
                    )?;
                    val
                }
                ParamSlot::Passthrough { index } => {
                    mt_fn.get_nth_param(index).ok_or_else(|| {
                        CodeGenError::TypeError(format!(
                            "decomposed entry '{mt_name}' missing param {index}"
                        ))
                    })?
                }
            };
            self.compilation
                .env
                .insert(pname.clone(), (param_val, (*pty).clone()));
        }
        Ok(())
    }

    /// Eager-load a Class-P indirect parameter's aggregate value from its
    /// caller-owned buffer `ptr` at `buf_index` (ADR 1.7.26e §2.6). One load at
    /// entry; the body then uses the resulting SSA value.
    fn load_indirect_param(
        &mut self,
        mt_fn: inkwell::values::FunctionValue<'ctx>,
        mt_name: &str,
        pname: &str,
        pty: &tungsten_core::types::Type,
        buf_index: u32,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let buf = mt_fn
            .get_nth_param(buf_index)
            .ok_or_else(|| {
                CodeGenError::TypeError(format!(
                    "Class-P entry '{mt_name}' missing indirect buffer {buf_index}"
                ))
            })?
            .into_pointer_value();
        let llvm_ty = self.types.lower_type(pty);
        self.builder
            .build_load(llvm_ty, buf, &format!("{pname}.indirect.load"))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))
    }

    /// Store a `$direct_mt` body result through the out-pointer (param 0) and
    /// emit `ret void` (ADR 1.7.26a, sret-style return). No-op if the current
    /// block is already terminated — e.g. a tail-recursive branch that already
    /// emitted `musttail … ; ret void`, leaving the builder in a dead block.
    pub(crate) fn emit_sret_return(
        &self,
        mt_fn: inkwell::values::FunctionValue<'ctx>,
        mt_name: &str,
        result: &BasicValueEnum<'ctx>,
    ) -> Result<(), CodeGenError> {
        let terminated = self
            .builder
            .get_insert_block()
            .and_then(inkwell::basic_block::BasicBlock::get_terminator)
            .is_some();
        if terminated {
            return Ok(());
        }
        let out_ptr = mt_fn
            .get_nth_param(0)
            .ok_or_else(|| {
                CodeGenError::TypeError(format!("sret $direct_mt '{mt_name}' missing out-pointer"))
            })?
            .into_pointer_value();
        self.builder
            .build_store(out_ptr, *result)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder
            .build_return(None)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
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
}
