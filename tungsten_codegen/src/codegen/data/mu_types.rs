//! Shared helpers for μ-type (recursive type) handling.
//!
//! μ-types are represented as opaque pointers at the LLVM level.
//! The underlying data is heap-allocated via malloc.
//!
//! - `fold` allocates the inner struct on the heap and returns a pointer
//! - `unfold` dereferences the pointer to get the inner struct
//!
//! This module provides shared utilities used by both sums.rs and adt.rs
//! to avoid code duplication.

use crate::codegen::error::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValue, BasicValueEnum, PointerValue};
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Heap-allocate a value and return a pointer to it.
    ///
    /// This is the core of `fold` for μ-types: allocate the inner struct
    /// on the heap and return an opaque pointer.
    ///
    /// # Arguments
    /// * `value` - The value to heap-allocate
    /// * `ty` - The LLVM type of the value (used for size calculation)
    /// * `alignment` - Alignment for the store (typically 16 for ARM64)
    ///
    /// # Returns
    /// A pointer to the heap-allocated value.
    pub(crate) fn fold_to_heap(
        &mut self,
        value: BasicValueEnum<'ctx>,
        ty: BasicTypeEnum<'ctx>,
        alignment: u32,
    ) -> Result<PointerValue<'ctx>, CodeGenError> {
        // Calculate size of the struct
        let size = self.type_size_bytes(ty);
        let i64_type = self.context.i64_type();
        let size_val = i64_type.const_int(size, false);

        // Get malloc function
        let malloc_fn = self
            .module
            .get_function("malloc")
            .ok_or_else(|| CodeGenError::LlvmError("malloc not declared".to_string()))?;

        // Allocate on heap
        let ptr = self
            .builder
            .build_call(malloc_fn, &[size_val.into()], "mu_alloc")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::LlvmError("malloc returned void".to_string()))?
            .into_pointer_value();

        // Store the value with specified alignment
        let store = self
            .builder
            .build_store(ptr, value)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        let _ = store.set_alignment(alignment);

        Ok(ptr)
    }

    /// Load a value from a μ-type pointer.
    ///
    /// This is the core of `unfold`: dereference the pointer to get
    /// the underlying struct value.
    ///
    /// # Arguments
    /// * `ptr` - Pointer to the heap-allocated value
    /// * `inner_ty` - The LLVM type to load
    /// * `alignment` - Alignment for the load (typically 16 for ARM64)
    /// * `name` - Name for the loaded value (for LLVM IR readability)
    ///
    /// # Returns
    /// The loaded value.
    pub(crate) fn load_mu_value(
        &mut self,
        ptr: PointerValue<'ctx>,
        inner_ty: BasicTypeEnum<'ctx>,
        alignment: u32,
        name: &str,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let loaded = self
            .builder
            .build_load(inner_ty, ptr, name)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        if let Some(inst) = loaded.as_instruction_value() {
            let _ = inst.set_alignment(alignment);
        }

        Ok(loaded)
    }

    /// Substitute a type for a variable in a type expression.
    ///
    /// Used for μ-type handling: when unfolding μ X. F[X], we need to
    /// substitute (μ X. F[X]) for X in F to get F[μ X. F[X]].
    pub(crate) fn substitute_type(&self, ty: &Type, var: &str, replacement: &Type) -> Type {
        match ty {
            Type::TyVar(v) if v == var => replacement.clone(),
            Type::TyVar(_) => ty.clone(),

            // Terminal types
            Type::Unit | Type::Bool | Type::Nat | Type::String | Type::Void | Type::Prop => {
                ty.clone()
            }

            // Composite types - recurse into components
            Type::Arrow(a, b) => Type::Arrow(
                Box::new(self.substitute_type(a, var, replacement)),
                Box::new(self.substitute_type(b, var, replacement)),
            ),
            Type::Product(a, b) => Type::Product(
                Box::new(self.substitute_type(a, var, replacement)),
                Box::new(self.substitute_type(b, var, replacement)),
            ),
            Type::Sum(a, b) => Type::Sum(
                Box::new(self.substitute_type(a, var, replacement)),
                Box::new(self.substitute_type(b, var, replacement)),
            ),

            // Binding forms - check for shadowing
            Type::Forall(v, body) if v == var => ty.clone(),
            Type::Forall(v, body) => Type::Forall(
                v.clone(),
                Box::new(self.substitute_type(body, var, replacement)),
            ),
            Type::Mu(v, body) if v == var => ty.clone(),
            Type::Mu(v, body) => Type::Mu(
                v.clone(),
                Box::new(self.substitute_type(body, var, replacement)),
            ),

            // Equality proofs
            Type::Eq(ty_inner, t1, t2) => Type::Eq(
                Box::new(self.substitute_type(ty_inner, var, replacement)),
                t1.clone(),
                t2.clone(),
            ),

            // Pointers and References
            Type::Ptr(inner) => Type::Ptr(Box::new(self.substitute_type(inner, var, replacement))),
            Type::Ref(inner) => Type::Ref(Box::new(self.substitute_type(inner, var, replacement))),

            // Type applications
            Type::App(name, args) => Type::App(
                name.clone(),
                args.iter()
                    .map(|a| self.substitute_type(a, var, replacement))
                    .collect(),
            ),

            // Flat ADT representation
            Type::Adt(name, type_args, variants) => Type::Adt(
                name.clone(),
                type_args
                    .iter()
                    .map(|a| self.substitute_type(a, var, replacement))
                    .collect(),
                variants
                    .iter()
                    .map(|(vname, vty)| {
                        (vname.clone(), self.substitute_type(vty, var, replacement))
                    })
                    .collect(),
            ),
        }
    }

    /// Unwrap a μ-type to get its underlying type with the μ-variable substituted.
    ///
    /// For μ X. F[X], returns F[μ X. F[X]] (the unfolding).
    /// For non-μ types, returns the type unchanged.
    pub(crate) fn unwrap_mu_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Mu(var, body) => {
                // Substitute the μ-type itself for the bound variable
                self.substitute_type(body, var, ty)
            }
            _ => ty.clone(),
        }
    }
}
