//! Product type (Pair) compilation.
//!
//! Handles compilation of pairs/tuples:
//! - Pair construction
//! - First projection (Fst)
//! - Second projection (Snd)

use crate::codegen::error::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicValue, BasicValueEnum};
use tungsten_core::terms::Term;

impl<'ctx> CodeGen<'ctx> {
    /// Compile a pair/tuple `(t1, t2)`.
    pub(crate) fn compile_pair(
        &mut self,
        t1: &Term,
        t2: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let v1 = self.compile_term(t1)?;
        let v2 = self.compile_term(t2)?;

        let pair_type = self
            .context
            .struct_type(&[v1.get_type(), v2.get_type()], false);
        let mut pair = pair_type.const_zero();
        pair = self
            .builder
            .build_insert_value(pair, v1, 0, "pair_fst")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();
        pair = self
            .builder
            .build_insert_value(pair, v2, 1, "pair_snd")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();

        Ok(pair.into())
    }

    /// Compile first projection `fst(t)`.
    ///
    /// Uses alloca + store + GEP + load for robustness with large structs
    /// on platforms like ARM64 where sret ABI applies.
    pub(crate) fn compile_fst(&mut self, t: &Term) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let pair = self.compile_term(t)?;
        // Get the struct type for the pair
        let pair_ty = self.infer_term_type(t)?;
        let struct_llvm_ty = self.types.lower_type(&pair_ty);

        // Handle both struct values and pointers to structs
        let struct_ptr = match pair {
            BasicValueEnum::StructValue(s) => {
                // Alloca space and store the struct with 16-byte alignment for ARM64
                let ptr = self
                    .builder
                    .build_alloca(struct_llvm_ty, "pair_alloca")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                if let Some(inst) = ptr.as_instruction() {
                    let _ = inst.set_alignment(16);
                }
                let store = self
                    .builder
                    .build_store(ptr, s)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                let _ = store.set_alignment(16);
                ptr
            }
            BasicValueEnum::PointerValue(p) => p,
            other => {
                return Err(CodeGenError::TypeError(format!(
                    "fst: expected struct or pointer, got {:?}",
                    other
                )))
            }
        };

        // GEP to field 0 and load
        let fst_ptr = self
            .builder
            .build_struct_gep(struct_llvm_ty, struct_ptr, 0, "fst_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Get the type of field 0
        let fst_ty = struct_llvm_ty
            .into_struct_type()
            .get_field_type_at_index(0)
            .ok_or_else(|| CodeGenError::TypeError("Fst: struct has no field 0".into()))?;

        let fst = self
            .builder
            .build_load(fst_ty, fst_ptr, "fst")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        // Set 16-byte alignment on load for ARM64 ABI
        if let Some(inst) = fst.as_instruction_value() {
            let _ = inst.set_alignment(16);
        }
        Ok(fst)
    }

    /// Compile second projection `snd(t)`.
    ///
    /// Uses alloca + store + GEP + load for robustness with large structs
    /// on platforms like ARM64 where sret ABI applies.
    pub(crate) fn compile_snd(&mut self, t: &Term) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let pair = self.compile_term(t)?;
        // Get the struct type for the pair
        let pair_ty = self.infer_term_type(t)?;
        let struct_llvm_ty = self.types.lower_type(&pair_ty);

        // Handle both struct values and pointers to structs
        let struct_ptr = match pair {
            BasicValueEnum::StructValue(s) => {
                // Alloca space and store the struct with 16-byte alignment for ARM64
                let ptr = self
                    .builder
                    .build_alloca(struct_llvm_ty, "pair_alloca")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                if let Some(inst) = ptr.as_instruction() {
                    let _ = inst.set_alignment(16);
                }
                let store = self
                    .builder
                    .build_store(ptr, s)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                let _ = store.set_alignment(16);
                ptr
            }
            BasicValueEnum::PointerValue(p) => p,
            other => {
                return Err(CodeGenError::TypeError(format!(
                    "snd: expected struct or pointer, got {:?}",
                    other
                )))
            }
        };

        // GEP to field 1 and load
        let snd_ptr = self
            .builder
            .build_struct_gep(struct_llvm_ty, struct_ptr, 1, "snd_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Get the type of field 1
        let snd_ty = struct_llvm_ty
            .into_struct_type()
            .get_field_type_at_index(1)
            .ok_or_else(|| CodeGenError::TypeError("Snd: struct has no field 1".into()))?;

        let snd = self
            .builder
            .build_load(snd_ty, snd_ptr, "snd")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        // Set 16-byte alignment on load for ARM64 ABI
        if let Some(inst) = snd.as_instruction_value() {
            let _ = inst.set_alignment(16);
        }
        Ok(snd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;
    use tungsten_core::terms::Term;

    fn setup_codegen_with_function(context: &Context) -> CodeGen {
        let mut codegen = CodeGen::new(context, "test");

        // Create a simple function to provide a basic block context
        let void_type = context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        let function = codegen.module.add_function("test_fn", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        codegen.builder.position_at_end(entry);
        codegen.current_fn = Some(function);

        codegen
    }

    #[test]
    fn test_compile_pair_nat_nat() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create a pair of (1, 2)
        let t1 = Term::NatLit(1);
        let t2 = Term::NatLit(2);

        let result = codegen.compile_pair(&t1, &t2).unwrap();
        assert!(result.is_struct_value());

        let struct_val = result.into_struct_value();
        assert_eq!(struct_val.get_type().count_fields(), 2);
    }

    #[test]
    fn test_compile_pair_nested() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create ((1, 2), 3)
        let inner = Term::Pair(Box::new(Term::NatLit(1)), Box::new(Term::NatLit(2)));
        let outer = Term::Pair(Box::new(inner), Box::new(Term::NatLit(3)));

        let result = codegen.compile_term(&outer).unwrap();
        assert!(result.is_struct_value());

        let struct_val = result.into_struct_value();
        assert_eq!(struct_val.get_type().count_fields(), 2);
    }

    #[test]
    fn test_compile_pair_bool_nat() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create (true, 42)
        let result = codegen
            .compile_pair(&Term::True, &Term::NatLit(42))
            .unwrap();
        assert!(result.is_struct_value());
        assert_eq!(result.into_struct_value().get_type().count_fields(), 2);
    }
}
