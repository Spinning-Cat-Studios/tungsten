//! Flat ADT compilation - direct tag + payload representation (ADR 2.2.26).
//!
//! # Flat ADT representation
//!
//! ADT types with n≥3 constructors are represented as `{ i32 tag, [max_payload x i8] data }`:
//! - `tag`: Constructor index (0, 1, 2, ..., n-1)
//! - `data`: Byte array large enough to hold the largest payload
//!
//! This provides O(1) variant dispatch via LLVM `switch` instruction, avoiding the
//! O(n) nested case expressions from right-nested binary sums.
//!
//! # Why i32 tag?
//!
//! Using i32 (vs i8) ensures:
//! - Support for ADTs with >256 constructors
//! - Natural alignment on modern architectures
//! - Consistent with Rust's enum discriminant

use crate::codegen::error::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::{BasicValue, BasicValueEnum};
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile ADT constructor application: AdtConstruct(adt_ty, variant_idx, payload)
    ///
    /// Generates:
    /// 1. Allocate `{ i32, [max_payload x i8] }` on stack
    /// 2. Store tag = variant_idx
    /// 3. Store payload into data field (bitcast to payload type)
    /// 4. Load and return the complete struct
    ///
    /// NOTE: This function NEVER folds/heap-allocates. For recursive ADTs, the
    /// elaborator wraps constructor calls in Term::Fold, which handles heap
    /// allocation via compile_fold. compile_adt_construct only builds the struct value.
    pub(crate) fn compile_adt_construct(
        &mut self,
        adt_ty: &Type,
        variant_idx: usize,
        payload: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Compile the payload first
        let payload_val = self.compile_term(payload)?;

        // Unwrap μ-type if present to get the actual ADT type
        // This handles recursive types like μ X. Adt(...)
        let unwrapped = self.unwrap_mu_type(adt_ty);

        // Use resolve_to_flat_adt to get the Type::Adt representation
        // (not the Sum encoding from expand_type)
        let actual_adt_ty = self.types.resolve_to_flat_adt(&unwrapped).ok_or_else(|| {
            CodeGenError::TypeError(format!(
                "compile_adt_construct requires Adt type, got {:?} (from {:?})",
                unwrapped, adt_ty
            ))
        })?;

        // Get the ADT's LLVM struct type: { i32, [N x i8] }
        let adt_llvm_ty = self.types.lower_type(&actual_adt_ty).into_struct_type();
        let i32_type = self.context.i32_type();

        // Allocate the ADT struct on stack with 16-byte alignment for ARM64 ABI
        let adt_ptr = self
            .builder
            .build_alloca(adt_llvm_ty, "adt_alloca")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = adt_ptr.as_instruction() {
            let _ = inst.set_alignment(16);
        }

        // Store tag = variant_idx at field 0
        let tag_ptr = self
            .builder
            .build_struct_gep(adt_llvm_ty, adt_ptr, 0, "tag_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        self.builder
            .build_store(tag_ptr, i32_type.const_int(variant_idx as u64, false))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Store payload into data field (field 1) via pointer cast
        // NOTE: data_ptr is at offset 4 from struct base, so max valid alignment is 4
        let data_ptr = self
            .builder
            .build_struct_gep(adt_llvm_ty, adt_ptr, 1, "data_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let store = self
            .builder
            .build_store(data_ptr, payload_val)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(4);

        // Load the complete struct value
        let adt_val = self
            .builder
            .build_load(adt_llvm_ty, adt_ptr, "adt_val")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = adt_val.as_instruction_value() {
            let _ = inst.set_alignment(16);
        }

        // Return the struct value directly.
        // For recursive ADTs, the elaborator wraps this in Term::Fold,
        // which calls compile_fold to handle heap allocation.
        Ok(adt_val)
    }

    /// Compile ADT pattern matching: AdtMatch(scrutinee, arms)
    ///
    /// Each arm is (variant_idx, var_name, body).
    ///
    /// Generates:
    /// 1. Extract tag from scrutinee
    /// 2. Build LLVM `switch` on tag value
    /// 3. For each arm: load payload, bind variable, compile body
    /// 4. Merge results via phi node
    pub(crate) fn compile_adt_match(
        &mut self,
        scrutinee: &Term,
        arms: &[(usize, String, Box<Term>)],
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Prepare scrutinee: compile, infer type, unwrap μ-type if needed
        let scrut_val = self.compile_term(scrutinee)?;
        let scrut_ty = self.infer_term_type(scrutinee)?;
        let (actual_scrut_ty, scrut_struct) = self.unwrap_adt_scrutinee(&scrut_ty, scrut_val)?;

        // Get ADT layout info
        let adt_llvm_ty = self.types.lower_type(&actual_scrut_ty).into_struct_type();
        let variants = self.get_adt_variants(&actual_scrut_ty)?;

        // Store scrutinee and extract tag + data pointer
        let (tag, data_ptr) = self.store_and_extract_adt_fields(scrut_struct, adt_llvm_ty)?;

        // Build switch with basic blocks for each arm
        let function = self
            .current_fn
            .ok_or_else(|| CodeGenError::LlvmError("no current function".to_string()))?;
        let (merge_bb, switch_info) = self.build_adt_switch(function, tag, arms)?;

        // Compile each arm and collect phi incoming values
        let phi_incoming =
            self.compile_adt_arms(&switch_info, arms, &variants, data_ptr, merge_bb)?;

        // Build merge block with phi
        self.build_adt_merge_phi(merge_bb, &phi_incoming)
    }

    /// Store ADT scrutinee on stack and extract tag + data pointer.
    fn store_and_extract_adt_fields(
        &mut self,
        scrut_struct: inkwell::values::StructValue<'ctx>,
        adt_llvm_ty: inkwell::types::StructType<'ctx>,
    ) -> Result<
        (
            inkwell::values::IntValue<'ctx>,
            inkwell::values::PointerValue<'ctx>,
        ),
        CodeGenError,
    > {
        // Allocate scrutinee on stack with 16-byte alignment
        let scrut_ptr = self
            .builder
            .build_alloca(adt_llvm_ty, "adt_scrut_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = scrut_ptr.as_instruction() {
            let _ = inst.set_alignment(16);
        }
        let store = self
            .builder
            .build_store(scrut_ptr, scrut_struct)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(16);

        // Extract tag (field 0)
        let i32_type = self.context.i32_type();
        let tag_ptr = self
            .builder
            .build_struct_gep(adt_llvm_ty, scrut_ptr, 0, "tag_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let tag = self
            .builder
            .build_load(i32_type, tag_ptr, "tag")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_int_value();

        // Get pointer to data field (field 1)
        let data_ptr = self
            .builder
            .build_struct_gep(adt_llvm_ty, scrut_ptr, 1, "data_ptr")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok((tag, data_ptr))
    }

    /// Build switch instruction with basic blocks for each arm.
    /// Returns the merge block and info about each arm's block.
    fn build_adt_switch(
        &mut self,
        function: inkwell::values::FunctionValue<'ctx>,
        tag: inkwell::values::IntValue<'ctx>,
        arms: &[(usize, String, Box<Term>)],
    ) -> Result<
        (
            inkwell::basic_block::BasicBlock<'ctx>,
            Vec<(inkwell::basic_block::BasicBlock<'ctx>, String, usize)>,
        ),
        CodeGenError,
    > {
        let i32_type = self.context.i32_type();
        let merge_bb = self.context.append_basic_block(function, "adt_merge");
        let default_bb = self.context.append_basic_block(function, "adt_unreachable");

        // Create blocks and switch cases for each arm
        let mut arm_blocks = Vec::with_capacity(arms.len());
        let mut switch_cases = Vec::with_capacity(arms.len());

        for (variant_idx, var_name, _) in arms {
            let bb = self
                .context
                .append_basic_block(function, &format!("adt_case_{}", variant_idx));
            arm_blocks.push((bb, var_name.clone(), *variant_idx));
            switch_cases.push((i32_type.const_int(*variant_idx as u64, false), bb));
        }

        // Build switch instruction
        self.builder
            .build_switch(tag, default_bb, &switch_cases)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        // Default case: unreachable (exhaustive match)
        self.builder.position_at_end(default_bb);
        self.builder
            .build_unreachable()
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok((merge_bb, arm_blocks))
    }

    /// Compile all ADT match arms and collect phi incoming values.
    fn compile_adt_arms(
        &mut self,
        arm_blocks: &[(inkwell::basic_block::BasicBlock<'ctx>, String, usize)],
        arms: &[(usize, String, Box<Term>)],
        variants: &[(String, Type)],
        data_ptr: inkwell::values::PointerValue<'ctx>,
        merge_bb: inkwell::basic_block::BasicBlock<'ctx>,
    ) -> Result<Vec<(BasicValueEnum<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)>, CodeGenError>
    {
        let mut phi_incoming = Vec::with_capacity(arms.len());
        let mut result_type: Option<inkwell::types::BasicTypeEnum<'ctx>> = None;

        for (i, (bb, var_name, variant_idx)) in arm_blocks.iter().enumerate() {
            let (arm_result, end_bb) = self.compile_adt_arm(
                *bb,
                data_ptr,
                variants,
                *variant_idx,
                var_name,
                &arms[i].2,
                &mut result_type,
                merge_bb,
            )?;
            phi_incoming.push((arm_result, end_bb));
        }

        Ok(phi_incoming)
    }

    /// Compile a single ADT match arm.
    fn compile_adt_arm(
        &mut self,
        bb: inkwell::basic_block::BasicBlock<'ctx>,
        data_ptr: inkwell::values::PointerValue<'ctx>,
        variants: &[(String, Type)],
        variant_idx: usize,
        var_name: &str,
        body: &Term,
        result_type: &mut Option<inkwell::types::BasicTypeEnum<'ctx>>,
        merge_bb: inkwell::basic_block::BasicBlock<'ctx>,
    ) -> Result<(BasicValueEnum<'ctx>, inkwell::basic_block::BasicBlock<'ctx>), CodeGenError> {
        self.builder.position_at_end(bb);

        // Get payload type for this variant
        let payload_ty = variants
            .get(variant_idx)
            .map(|(_, ty)| ty.clone())
            .unwrap_or(Type::Unit);
        let payload_llvm_ty = self.types.lower_type(&payload_ty);

        // Load payload with 4-byte alignment (data is at offset 4)
        let payload_val = self
            .builder
            .build_load(
                payload_llvm_ty,
                data_ptr,
                &format!("payload_{}", variant_idx),
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        if let Some(inst) = payload_val.as_instruction_value() {
            let _ = inst.set_alignment(4);
        }

        // Bind variable, compile body, restore env
        let old_binding = self
            .env
            .insert(var_name.to_string(), (payload_val, payload_ty));
        let arm_result = self.compile_term(body)?;
        if let Some(v) = old_binding {
            self.env.insert(var_name.to_string(), v);
        } else {
            self.env.remove(var_name);
        }

        // Track/cast result type for phi consistency
        if result_type.is_none() {
            *result_type = Some(arm_result.get_type());
        }
        let arm_result = if let Some(expected_ty) = result_type {
            if arm_result.get_type() != *expected_ty {
                self.cast_to_type(arm_result, *expected_ty)?
            } else {
                arm_result
            }
        } else {
            arm_result
        };

        // Branch to merge
        let actual_end_bb = self.builder.get_insert_block().unwrap();
        self.builder
            .build_unconditional_branch(merge_bb)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok((arm_result, actual_end_bb))
    }

    /// Build merge block with phi node for ADT match results.
    fn build_adt_merge_phi(
        &mut self,
        merge_bb: inkwell::basic_block::BasicBlock<'ctx>,
        phi_incoming: &[(BasicValueEnum<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)],
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        self.builder.position_at_end(merge_bb);

        let result_type = phi_incoming
            .first()
            .map(|(val, _)| val.get_type())
            .ok_or_else(|| CodeGenError::TypeError("ADT match has no arms".to_string()))?;

        let phi = self
            .builder
            .build_phi(result_type, "adt_result")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        for (val, bb) in phi_incoming {
            phi.add_incoming(&[(val, *bb)]);
        }

        Ok(phi.as_basic_value())
    }

    /// Unwrap μ-type wrapper and return the actual ADT type + struct value.
    fn unwrap_adt_scrutinee(
        &mut self,
        ty: &Type,
        val: BasicValueEnum<'ctx>,
    ) -> Result<(Type, inkwell::values::StructValue<'ctx>), CodeGenError> {
        match ty {
            Type::Mu(_, _) => {
                // For μ X. Adt(...), unwrap and load from pointer
                let inner_ty = self.unwrap_mu_type(ty);

                // Resolve to flat ADT if inner is a type variable
                let resolved_inner = self
                    .types
                    .resolve_to_flat_adt(&inner_ty)
                    .unwrap_or(inner_ty);

                let inner_llvm_ty = self.types.lower_type(&resolved_inner);

                let ptr = val.into_pointer_value();
                // Use shared μ-type helper: load the struct value from the pointer
                let loaded = self.load_mu_value(ptr, inner_llvm_ty, 16, "unfolded_adt")?;

                Ok((resolved_inner, loaded.into_struct_value()))
            }
            Type::Adt(_, _, _) => Ok((ty.clone(), val.into_struct_value())),
            Type::TyVar(_) | Type::App(_, _) => {
                // Try to resolve to flat ADT
                if let Some(adt_ty) = self.types.resolve_to_flat_adt(ty) {
                    Ok((adt_ty, val.into_struct_value()))
                } else {
                    Err(CodeGenError::TypeError(format!(
                        "expected ADT type in match, got {:?}",
                        ty
                    )))
                }
            }
            _ => Err(CodeGenError::TypeError(format!(
                "expected ADT type in match, got {:?}",
                ty
            ))),
        }
    }

    /// Extract variant info from an ADT type.
    fn get_adt_variants(&self, ty: &Type) -> Result<Vec<(String, Type)>, CodeGenError> {
        match ty {
            Type::Adt(_, _, variants) => Ok(variants.clone()),
            _ => Err(CodeGenError::TypeError(format!(
                "expected Type::Adt, got {:?}",
                ty
            ))),
        }
    }

    /// Check if a type contains a specific type variable (TyVar).
    /// Used to detect recursive ADTs by checking if variants reference α_{adt_name}.
    fn type_contains_tyvar(ty: &Type, var_name: &str) -> bool {
        match ty {
            Type::TyVar(name) => name == var_name,
            Type::Arrow(t1, t2) => {
                Self::type_contains_tyvar(t1, var_name) || Self::type_contains_tyvar(t2, var_name)
            }
            Type::Product(t1, t2) => {
                Self::type_contains_tyvar(t1, var_name) || Self::type_contains_tyvar(t2, var_name)
            }
            Type::Sum(t1, t2) => {
                Self::type_contains_tyvar(t1, var_name) || Self::type_contains_tyvar(t2, var_name)
            }
            Type::Mu(_, inner) => Self::type_contains_tyvar(inner, var_name),
            Type::App(_, type_args) => type_args
                .iter()
                .any(|t| Self::type_contains_tyvar(t, var_name)),
            Type::Forall(_, inner) => Self::type_contains_tyvar(inner, var_name),
            Type::Adt(_, _, variants) => variants
                .iter()
                .any(|(_, payload_ty)| Self::type_contains_tyvar(payload_ty, var_name)),
            // Terminal types
            Type::Nat | Type::Bool | Type::String | Type::Unit | Type::Ref(_) => false,
            // Eq, Ptr - unlikely to contain recursive refs but handle them
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;
    use tungsten_core::terms::Term;

    fn setup_codegen_with_function(context: &Context) -> CodeGen<'_> {
        let mut codegen = CodeGen::new(context, "test");

        // Create a simple function to provide a basic block context
        let void_type = context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        let function = codegen.module.add_function("test_fn", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        codegen.builder.position_at_end(entry);
        codegen.current_fn = Some(function);

        // Declare malloc for fold operations (recursive ADTs)
        let i64_type = context.i64_type();
        let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
        let malloc_type = ptr_type.fn_type(&[i64_type.into()], false);
        codegen.module.add_function("malloc", malloc_type, None);

        codegen
    }

    /// Create a simple non-recursive ADT type for testing:
    /// type Color = | Red(Unit) | Green(Unit) | Blue(Unit)
    fn make_color_adt() -> Type {
        Type::Adt(
            "Color".to_string(),
            vec![], // no type args
            vec![
                ("Red".to_string(), Type::Unit),
                ("Green".to_string(), Type::Unit),
                ("Blue".to_string(), Type::Unit),
            ],
        )
    }

    /// Create a simple ADT with different payload types:
    /// type Shape = | Circle(Nat) | Rectangle(Nat, Nat) | Triangle(Bool)
    fn make_shape_adt() -> Type {
        Type::Adt(
            "Shape".to_string(),
            vec![],
            vec![
                ("Circle".to_string(), Type::Nat), // radius
                (
                    "Rectangle".to_string(),
                    Type::Product(Box::new(Type::Nat), Box::new(Type::Nat)),
                ), // width, height
                ("Triangle".to_string(), Type::Bool), // is_equilateral
            ],
        )
    }

    #[test]
    fn test_adt_type_layout_i32_tag() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // ADT type with inline variant info - no registration needed
        let adt_ty = make_color_adt();
        let llvm_ty = codegen.types.lower_type(&adt_ty).into_struct_type();

        // Should have 2 fields: i32 tag + data array
        assert_eq!(llvm_ty.count_fields(), 2);

        // First field should be i32 (tag)
        let tag_type = llvm_ty.get_field_type_at_index(0).unwrap();
        assert!(tag_type.is_int_type());
        assert_eq!(tag_type.into_int_type().get_bit_width(), 32);

        // Second field should be array (data)
        let data_type = llvm_ty.get_field_type_at_index(1).unwrap();
        assert!(data_type.is_array_type());
    }

    #[test]
    fn test_compile_adt_construct_simple() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Create the Color ADT - variants are inline in the type
        let adt_ty = make_color_adt();

        // Construct Color::Green(())
        let payload = Term::Unit;
        let result = codegen.compile_adt_construct(&adt_ty, 1, &payload).unwrap();

        // Result should be a struct value
        assert!(result.is_struct_value());

        // Struct should have 2 fields
        let struct_val = result.into_struct_value();
        assert_eq!(struct_val.get_type().count_fields(), 2);
    }

    #[test]
    fn test_compile_adt_construct_with_nat_payload() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Shape ADT - variants are inline in the type
        let adt_ty = make_shape_adt();

        // Construct Shape::Circle(42)
        let payload = Term::NatLit(42);
        let result = codegen.compile_adt_construct(&adt_ty, 0, &payload).unwrap();

        // Result should be a struct value
        assert!(result.is_struct_value());
    }

    #[test]
    fn test_get_adt_variants() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let adt_ty = make_color_adt();
        let variants = codegen.get_adt_variants(&adt_ty).unwrap();

        // Should have 3 variants
        assert_eq!(variants.len(), 3);

        // Check variant names and payload types
        assert_eq!(variants[0].0, "Red");
        assert_eq!(variants[0].1, Type::Unit);

        assert_eq!(variants[1].0, "Green");
        assert_eq!(variants[1].1, Type::Unit);

        assert_eq!(variants[2].0, "Blue");
        assert_eq!(variants[2].1, Type::Unit);
    }

    #[test]
    fn test_get_adt_variants_non_adt_error() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        // Try to get variants from a non-ADT type
        let nat_ty = Type::Nat;
        let result = codegen.get_adt_variants(&nat_ty);

        // Should return an error
        assert!(result.is_err());
    }

    #[test]
    fn test_adt_match_exhaustive() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Color ADT - variants are inline in the type
        let adt_ty = make_color_adt();

        // Create scrutinee: Color::Red(())
        let scrut = Term::AdtConstruct(
            adt_ty.clone(),
            0, // Red
            Box::new(Term::Unit),
        );

        // Create exhaustive match arms that return Nat
        // match color { Red(_) => 0, Green(_) => 1, Blue(_) => 2 }
        let arms = vec![
            (0, "r".to_string(), Box::new(Term::NatLit(0))),
            (1, "g".to_string(), Box::new(Term::NatLit(1))),
            (2, "b".to_string(), Box::new(Term::NatLit(2))),
        ];

        let result = codegen.compile_adt_match(&scrut, &arms);

        // Should compile successfully
        assert!(result.is_ok());
    }

    #[test]
    fn test_adt_match_with_payload_binding() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        // Shape ADT with different payload types
        let adt_ty = make_shape_adt();

        // Create scrutinee: Shape::Circle(42)
        let scrut = Term::AdtConstruct(
            adt_ty.clone(),
            0, // Circle
            Box::new(Term::NatLit(42)),
        );

        // Match arms that use the bound payload
        // match shape { Circle(r) => r, Rectangle(dims) => 0, Triangle(b) => 1 }
        let arms = vec![
            (0, "r".to_string(), Box::new(Term::Var("r".to_string()))), // uses payload
            (1, "dims".to_string(), Box::new(Term::NatLit(0))),
            (2, "b".to_string(), Box::new(Term::NatLit(1))),
        ];

        let result = codegen.compile_adt_match(&scrut, &arms);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_adt_variants_shape() {
        let context = Context::create();
        let codegen = setup_codegen_with_function(&context);

        let adt_ty = make_shape_adt();
        let variants = codegen.get_adt_variants(&adt_ty).unwrap();

        assert_eq!(variants.len(), 3);

        // Verify variant names and payload types
        assert_eq!(variants[0].0, "Circle");
        assert_eq!(variants[0].1, Type::Nat);

        assert_eq!(variants[1].0, "Rectangle");
        assert!(matches!(variants[1].1, Type::Product(_, _)));

        assert_eq!(variants[2].0, "Triangle");
        assert_eq!(variants[2].1, Type::Bool);
    }

    #[test]
    fn test_compile_adt_construct_all_variants() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        let adt_ty = make_color_adt();

        // Test constructing all three variants
        for idx in [0, 1, 2] {
            let result = codegen.compile_adt_construct(&adt_ty, idx, &Term::Unit);
            assert!(
                result.is_ok(),
                "Failed to construct variant at index {}",
                idx
            );
            assert!(result.unwrap().is_struct_value());
        }
    }

    #[test]
    fn test_compile_adt_construct_with_product_payload() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        let adt_ty = make_shape_adt();

        // Construct Shape::Rectangle((10, 20))
        let payload = Term::Pair(Box::new(Term::NatLit(10)), Box::new(Term::NatLit(20)));
        let result = codegen.compile_adt_construct(&adt_ty, 1, &payload);

        assert!(result.is_ok());
        assert!(result.unwrap().is_struct_value());
    }

    #[test]
    fn test_type_contains_tyvar_simple() {
        // Test direct TyVar
        assert!(CodeGen::type_contains_tyvar(
            &Type::TyVar("X".to_string()),
            "X"
        ));
        assert!(!CodeGen::type_contains_tyvar(
            &Type::TyVar("Y".to_string()),
            "X"
        ));
    }

    #[test]
    fn test_type_contains_tyvar_nested() {
        // Test nested in Arrow
        let arrow_ty = Type::Arrow(Box::new(Type::TyVar("X".to_string())), Box::new(Type::Nat));
        assert!(CodeGen::type_contains_tyvar(&arrow_ty, "X"));
        assert!(!CodeGen::type_contains_tyvar(&arrow_ty, "Y"));

        // Test nested in Product
        let prod_ty = Type::Product(Box::new(Type::Nat), Box::new(Type::TyVar("X".to_string())));
        assert!(CodeGen::type_contains_tyvar(&prod_ty, "X"));
    }

    #[test]
    fn test_type_contains_tyvar_terminal_types() {
        // Terminal types should not contain any type variable
        assert!(!CodeGen::type_contains_tyvar(&Type::Nat, "X"));
        assert!(!CodeGen::type_contains_tyvar(&Type::Bool, "X"));
        assert!(!CodeGen::type_contains_tyvar(&Type::String, "X"));
        assert!(!CodeGen::type_contains_tyvar(&Type::Unit, "X"));
    }

    #[test]
    fn test_adt_match_partial_arms() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        let adt_ty = make_color_adt();

        // Create scrutinee: Color::Green(())
        let scrut = Term::AdtConstruct(
            adt_ty.clone(),
            1, // Green
            Box::new(Term::Unit),
        );

        // Only match on two variants (partial match - Red and Green only)
        // In practice, this should ideally be caught at type-check time,
        // but the codegen should still work
        let arms = vec![
            (0, "r".to_string(), Box::new(Term::NatLit(0))),
            (1, "g".to_string(), Box::new(Term::NatLit(1))),
        ];

        let result = codegen.compile_adt_match(&scrut, &arms);
        // Should compile (default case goes to unreachable)
        assert!(result.is_ok());
    }

    /// Create a 5-variant ADT to test larger switches:
    /// type Weekday = | Mon(Unit) | Tue(Unit) | Wed(Unit) | Thu(Unit) | Fri(Unit)
    fn make_weekday_adt() -> Type {
        Type::Adt(
            "Weekday".to_string(),
            vec![],
            vec![
                ("Mon".to_string(), Type::Unit),
                ("Tue".to_string(), Type::Unit),
                ("Wed".to_string(), Type::Unit),
                ("Thu".to_string(), Type::Unit),
                ("Fri".to_string(), Type::Unit),
            ],
        )
    }

    #[test]
    fn test_adt_match_five_variants() {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);

        let adt_ty = make_weekday_adt();

        // Create scrutinee: Weekday::Wed(())
        let scrut = Term::AdtConstruct(
            adt_ty.clone(),
            2, // Wed
            Box::new(Term::Unit),
        );

        // Match all five variants
        let arms = vec![
            (0, "m".to_string(), Box::new(Term::NatLit(1))),
            (1, "t".to_string(), Box::new(Term::NatLit(2))),
            (2, "w".to_string(), Box::new(Term::NatLit(3))),
            (3, "th".to_string(), Box::new(Term::NatLit(4))),
            (4, "f".to_string(), Box::new(Term::NatLit(5))),
        ];

        let result = codegen.compile_adt_match(&scrut, &arms);
        assert!(result.is_ok());
    }
}
