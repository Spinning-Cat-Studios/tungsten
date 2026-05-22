//! ABI safety checks and calling-convention helpers.
//!
//! Centralises platform-specific ABI constraints (e.g. `AArch64` musttail
//! restrictions) so that callsites in `exec/direct_calls/` and
//! `exec/closures/application.rs` share a single implementation.
//!
//! The guard rejects struct returns/params for all targets and call kinds.
//! LLVM 18 crashes LLC with "failed to perform tail call elimination" for
//! musttail + struct returns on both `x86_64` and `AArch64`, regardless of
//! whether the call is direct self-recursive or indirect closure dispatch.
//!
//! The target and call-kind enums are retained for documentation and
//! future LLVM versions that may relax these constraints.

use super::backend::CodeGenError;
use super::CodeGen;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValue, BasicValueEnum};

/// Target classification for musttail ABI safety decisions.
///
/// Controls whether struct returns/parameters are allowed with `musttail`.
/// See ADR 12.5.26e for rationale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MusttailAbiTarget {
    /// Empirically crashes same as `AArch64` for struct returns with musttail.
    /// Retained for future LLVM versions that may handle `x86_64` struct sret correctly.
    X86_64,
    /// Known crashes: LLVM 18 `SelectionDAGISel` aborts on struct ABI patterns.
    AArch64,
    /// Unvalidated: conservative rejection until empirically validated.
    Unknown,
}

/// Classify a target triple for musttail ABI decisions.
///
/// Splits on the architecture component (first segment of the triple).
pub(crate) fn classify_musttail_target(triple: &str) -> MusttailAbiTarget {
    let arch = triple.split('-').next().unwrap_or(triple);
    match arch {
        "x86_64" | "amd64" => MusttailAbiTarget::X86_64,
        "aarch64" | "arm64" => MusttailAbiTarget::AArch64,
        _ => MusttailAbiTarget::Unknown,
    }
}

/// Distinguishes the two musttail call paths for ABI safety decisions.
///
/// Retained for documentation and future LLVM versions. LLVM 18 on `AArch64`
/// rejects musttail with struct returns for **all** call kinds, so both
/// variants currently receive the same treatment. See ADR 12.5.26f §6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MusttailCallKind {
    /// Caller == callee (self-recursive), no function pointer indirection.
    /// LLVM 18 still rejects struct returns for direct calls on `AArch64`.
    DirectSelfRecursive,
    /// Indirect call through function pointer (closure application).
    /// Crashes LLVM 18 `SelectionDAGISel` on `AArch64` with struct returns.
    IndirectClosure,
}

impl<'ctx> CodeGen<'ctx> {
    /// Check whether a function type is compatible with `musttail` on the
    /// current target architecture and call kind.
    ///
    /// Returns `Ok(())` if safe, `Err(reason)` if musttail should be skipped.
    ///
    /// This helper is ABI-only: tail position, self-recursion, callee identity,
    /// and function-type equality are the caller's responsibility.
    ///
    /// The guard rejects struct returns/params uniformly — LLVM 18 crashes
    /// for musttail + struct on all validated targets (`x86_64`, `AArch64`).
    /// The `target/call_kind` parameters are retained for future LLVM versions.
    pub(crate) fn check_musttail_abi_safety(
        &self,
        fn_type: inkwell::types::FunctionType<'ctx>,
        call_kind: MusttailCallKind,
    ) -> Result<(), &'static str> {
        let triple = self.module.get_triple();
        // WARNING: Do NOT use triple.to_string() — inkwell's TargetTriple::to_string()
        // returns Debug format like `TargetTriple("x86_64-...")`, not the raw string.
        // Always use .as_str().to_string_lossy() to get the actual triple.
        let triple_str = triple.as_str().to_string_lossy();
        // Classification retained for future LLVM versions that may allow
        // struct returns on specific target/call-kind combinations.
        let _target = classify_musttail_target(&triple_str);
        _ = call_kind;

        // All targets: LLVM 18 rejects musttail with struct returns/params.
        if let Some(ret) = fn_type.get_return_type() {
            if ret.is_struct_type() {
                return Err("struct return (musttail incompatible in LLVM 18)");
            }
        }
        for param in fn_type.get_param_types() {
            if param.is_struct_type() {
                return Err("struct parameter (musttail incompatible in LLVM 18)");
            }
        }
        Ok(())
    }

    /// Check if a function type's struct parameters can be decomposed into
    /// scalars for musttail compatibility (ADR 18.5.26a).
    ///
    /// Returns `Some(flattened_param_types)` if decomposition is possible,
    /// `None` if any struct param is not flattenable. The returned list
    /// contains only the non-env parameters (skips param 0 which is always
    /// the env ptr).
    ///
    /// Flattening rules: only structs whose fields are all scalar (int, ptr,
    /// float). Rejects nested structs, arrays, and structs with >8 fields.
    pub(crate) fn check_decomposition_eligible(
        &self,
        fn_type: inkwell::types::FunctionType<'ctx>,
    ) -> Option<Vec<BasicTypeEnum<'ctx>>> {
        let mut has_struct_param = false;
        let mut flattened: Vec<BasicTypeEnum<'ctx>> = Vec::new();
        let params = fn_type.get_param_types();

        // Skip param 0 (env ptr) — it's always a scalar pointer
        for param in params.iter().skip(1) {
            if param.is_struct_type() {
                has_struct_param = true;
                let st = param.into_struct_type();
                let field_count = st.count_fields();
                if field_count > 8 {
                    return None; // too many fields
                }
                for i in 0..field_count {
                    let field_ty = st.get_field_type_at_index(i)?;
                    if field_ty.is_struct_type() || field_ty.is_array_type() {
                        return None; // nested struct or array
                    }
                    flattened.push(field_ty);
                }
            } else {
                flattened.push(*param);
            }
        }

        if has_struct_param {
            Some(flattened)
        } else {
            None // no struct params — nothing to decompose
        }
    }

    /// Materialize a struct value through memory if it's large.
    ///
    /// On ARM64, structs larger than 16 bytes are returned via sret.
    pub(crate) fn materialize_call_result(
        &self,
        value: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        if let BasicValueEnum::StructValue(sv) = value {
            let ty = sv.get_type();
            let size = self.type_size_bytes(ty.into());

            if size > 16 {
                let alloca = self
                    .builder
                    .build_alloca(ty, "call_result_alloca")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                if let Some(inst) = alloca.as_instruction() {
                    let _ = inst.set_alignment(16);
                }
                let store = self
                    .builder
                    .build_store(alloca, sv)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                let _ = store.set_alignment(16);
                let reloaded = self
                    .builder
                    .build_load(ty, alloca, "call_result_loaded")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                if let Some(inst) = reloaded.as_instruction_value() {
                    let _ = inst.set_alignment(16);
                }
                return Ok(reloaded);
            }
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_decompose;
#[cfg(test)]
mod tests_musttail;
