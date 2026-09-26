//! The `$direct` shim: keep the by-value ABI for external callers, then bridge
//! to the internal `$direct_mt` indirect ABI — allocate the sret buffer and one
//! buffer per Class-P indirect param, decompose flattenable structs, call
//! `$direct_mt`, and load the by-value result back (ADRs 18.5.26a, 1.7.26a,
//! 1.7.26e §2.4).
//!
//! `$direct` keeps its by-value struct params/return so *external* direct
//! callers are unaffected; only this shim + `$direct_mt` speak the indirect ABI.
//! The buffers live in the shim frame, which persists for the whole `$direct_mt`
//! musttail recursion, so the recursion is O(1) stack against one buffer each.

use super::{plan_mt_slots, ParamLowering, ParamSlot};
use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicMetadataValueEnum, PointerValue};

impl<'ctx> CodeGen<'ctx> {
    /// Emit the `$direct` shim that bridges the by-value ABI to `$direct_mt`.
    pub(crate) fn compile_decompose_shim(
        &mut self,
        name: &str,
        _mt_name: &str,
        mt_fn: inkwell::values::FunctionValue<'ctx>,
        param_map: &[ParamLowering],
    ) -> Result<(), CodeGenError> {
        let direct_name_str = super::super::helpers::direct_name(name);
        let direct_fn = self.module.get_function(&direct_name_str).ok_or_else(|| {
            CodeGenError::Unsupported(format!(
                "direct entry '{direct_name_str}' not declared for shim"
            ))
        })?;

        // $direct was declared but not compiled (we run instead of compile_direct_entry).
        let entry = self.context.append_basic_block(direct_fn, "entry");
        self.builder.position_at_end(entry);

        let sret = mt_fn.get_type().get_return_type().is_none();
        let plan = plan_mt_slots(param_map, sret);

        // Slot-indexed argument vector for the $direct_mt call.
        let n_slots = mt_fn.count_params() as usize;
        let mut args: Vec<Option<BasicMetadataValueEnum<'ctx>>> = vec![None; n_slots];

        // sret out-buffer (allocated here; the shim owns the result).
        let sret_buf = if sret {
            let ret_ty = direct_fn.get_type().get_return_type().ok_or_else(|| {
                CodeGenError::TypeError(format!(
                    "sret shim '{direct_name_str}' has no by-value return type"
                ))
            })?;
            let buf = self
                .builder
                .build_alloca(ret_ty, "sret_buf")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            self.emit_lifetime_marker("llvm.lifetime.start", buf, self.types.type_size(ret_ty))?;
            args[0] = Some(buf.into());
            Some((buf, ret_ty))
        } else {
            None
        };

        // env ptr passthrough.
        let env = direct_fn
            .get_nth_param(0)
            .ok_or_else(|| CodeGenError::TypeError("shim missing env ptr".to_string()))?;
        args[plan.env_index as usize] = Some(env.into());

        let indirect_bufs = self.fill_shim_slots(direct_fn, &direct_name_str, &plan, &mut args)?;

        let mt_args: Vec<BasicMetadataValueEnum<'ctx>> = args
            .into_iter()
            .enumerate()
            .map(|(idx, a)| {
                a.ok_or_else(|| {
                    CodeGenError::TypeError(format!("shim '{direct_name_str}' slot {idx} unfilled"))
                })
            })
            .collect::<Result<_, _>>()?;

        let call = self
            .builder
            .build_call(mt_fn, &mt_args, "shim_call")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        // R6: the call site must carry the same canonical slot attributes as the
        // `$direct_mt` declaration (`sret` in particular is ABI-impacting).
        if let Some(sig) = self.direct_calls.lowered_sig(name) {
            sig.attach_to_call_site(self.context, call);
        }

        // Buffers are dead once the call returns (and, for sret, once the
        // result is read back) — end their lifetimes so the backend can reuse
        // the frame space (R8, §2.5).
        for (buf, size) in indirect_bufs {
            self.emit_lifetime_marker("llvm.lifetime.end", buf, size)?;
        }

        let result = if let Some((buf, ret_ty)) = sret_buf {
            let loaded = self
                .builder
                .build_load(ret_ty, buf, "sret_load")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            self.emit_lifetime_marker("llvm.lifetime.end", buf, self.types.type_size(ret_ty))?;
            loaded
        } else {
            call.try_as_basic_value()
                .left()
                .ok_or_else(|| CodeGenError::TypeError("shim call returned void".to_string()))?
        };

        self.builder
            .build_return(Some(&result))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        self.verify_after_compile(&direct_name_str)?;

        Ok(())
    }

    /// Fill the per-source-param arg slots for the `$direct_mt` call: buffer each
    /// incoming by-value indirect struct (alloca + store → `ptr`), decompose
    /// flattenable structs into scalar fields, and pass others through.
    ///
    /// Returns the freshly allocated indirect buffers with their sizes so the
    /// caller can end their lifetimes after the `$direct_mt` call (R8).
    fn fill_shim_slots(
        &mut self,
        direct_fn: inkwell::values::FunctionValue<'ctx>,
        direct_name_str: &str,
        plan: &super::MtSlotPlan,
        args: &mut [Option<BasicMetadataValueEnum<'ctx>>],
    ) -> Result<Vec<(PointerValue<'ctx>, u64)>, CodeGenError> {
        let mut bufs = Vec::new();
        for (i, slot) in plan.slots.iter().enumerate() {
            let orig_idx = (i + 1) as u32; // $direct arg (env is 0)
            let param = direct_fn.get_nth_param(orig_idx).ok_or_else(|| {
                CodeGenError::TypeError(format!(
                    "shim '{direct_name_str}' missing param {orig_idx}"
                ))
            })?;
            match *slot {
                ParamSlot::Indirect { buf_index } => {
                    // Buffer the incoming by-value struct so $direct_mt gets a ptr.
                    let buf = self
                        .builder
                        .build_alloca(param.get_type(), &format!("indirect_buf.{i}"))
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                    let size = self.types.type_size(param.get_type());
                    self.emit_lifetime_marker("llvm.lifetime.start", buf, size)?;
                    self.builder
                        .build_store(buf, param)
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                    args[buf_index as usize] = Some(buf.into());
                    bufs.push((buf, size));
                }
                ParamSlot::Decompose { start, field_count } => {
                    let sv = param.into_struct_value();
                    for field_idx in 0..field_count {
                        let field = self
                            .builder
                            .build_extract_value(sv, field_idx, &format!("unpack.{i}.{field_idx}"))
                            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                        args[(start + field_idx) as usize] = Some(field.into());
                    }
                }
                ParamSlot::Passthrough { index } => {
                    args[index as usize] = Some(param.into());
                }
            }
        }
        Ok(bufs)
    }

    /// Emit an `llvm.lifetime.start`/`.end` marker for a shim buffer (R8, §2.5)
    /// so the backend can reuse frame space once the buffer is dead.
    fn emit_lifetime_marker(
        &self,
        intrinsic_name: &str,
        buf: PointerValue<'ctx>,
        size: u64,
    ) -> Result<(), CodeGenError> {
        let intrinsic = inkwell::intrinsics::Intrinsic::find(intrinsic_name).ok_or_else(|| {
            CodeGenError::LlvmError(format!("intrinsic '{intrinsic_name}' not found"))
        })?;
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let decl = intrinsic
            .get_declaration(&self.module, &[ptr_ty.into()])
            .ok_or_else(|| {
                CodeGenError::LlvmError(format!("cannot declare intrinsic '{intrinsic_name}'"))
            })?;
        let size_arg = self.context.i64_type().const_int(size, false);
        self.builder
            .build_call(decl, &[size_arg.into(), buf.into()], "")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(())
    }
}
