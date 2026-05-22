//! Type encoding and sizing — constructor payloads, ADT encoding, and size estimation.

use super::strip_named_prefix;
use super::CodegenConstructor;
use super::TypeLowering;
use inkwell::types::BasicTypeEnum;
use std::collections::HashMap;
use tungsten_core::types::Type;

impl<'ctx> TypeLowering<'ctx> {
    /// Get the size of an LLVM type in bytes.
    /// Uses LLVM `TargetData` when available for accurate alignment, falls back to conservative estimate.
    #[must_use]
    pub fn type_size(&self, ty: BasicTypeEnum<'ctx>) -> u64 {
        // Use TargetData for accurate size calculation including alignment padding
        if let Some(ref td) = self.target_data {
            return td.get_store_size(&ty);
        }
        // Fallback with conservative alignment padding
        self.type_size_fallback(ty)
    }

    /// Fallback size calculation with conservative alignment padding.
    /// Used when `TargetData` is not available.
    fn type_size_fallback(&self, ty: BasicTypeEnum<'ctx>) -> u64 {
        match ty {
            BasicTypeEnum::IntType(t) => u64::from(t.get_bit_width()).div_ceil(8),
            BasicTypeEnum::FloatType(_) => 8,
            BasicTypeEnum::PointerType(_) => 8,
            BasicTypeEnum::ArrayType(t) => {
                u64::from(t.len()) * self.type_size_fallback(t.get_element_type())
            }
            BasicTypeEnum::StructType(t) => {
                // Sum fields with 8-byte alignment padding between each field
                let mut size = 0u64;
                for field in t.get_field_types() {
                    // Align to 8 bytes before adding each field
                    size = (size + 7) & !7;
                    size += self.type_size_fallback(field);
                }
                // Round struct total to 16-byte alignment for ARM64
                (size + 15) & !15
            }
            BasicTypeEnum::VectorType(t) => {
                u64::from(t.get_size()) * self.type_size_fallback(t.get_element_type())
            }
        }
    }

    /// Compute max payload size for ADT variants iteratively.
    /// Uses fixed sizes for recursive/pointer types to avoid deep recursion.
    pub(super) fn compute_max_payload_size_iterative(
        &self,
        variants: &[(String, Type)],
        type_subst: &HashMap<String, Type>,
    ) -> u64 {
        let mut max_size = 0u64;

        for (_name, payload_ty) in variants {
            let size = self.estimate_type_size(payload_ty, type_subst);
            max_size = max_size.max(size);
        }

        max_size.max(1) // Minimum 1 byte for nullary variants
    }

    /// Compute the concrete LLVM type of the largest variant payload.
    ///
    /// W4 (ADR 11.4.26c): Returns the actual LLVM type rather than a byte count,
    /// so the struct definition carries type information for SROA.
    /// Only safe for non-recursive ADTs (recursive ADTs use ptr indirection).
    pub(super) fn compute_largest_payload_llvm_type(
        &mut self,
        variants: &[(String, Type)],
    ) -> BasicTypeEnum<'ctx> {
        let mut max_size = 0u64;
        let mut max_type: Option<BasicTypeEnum<'ctx>> = None;

        for (_name, payload_ty) in variants {
            let llvm_ty = self.lower_type(payload_ty);
            let size = self.type_size(llvm_ty);
            if size > max_size || max_type.is_none() {
                max_size = size;
                max_type = Some(llvm_ty);
            }
        }

        // Fallback for empty variant lists (shouldn't happen in practice)
        max_type.unwrap_or_else(|| self.context.i8_type().array_type(1).into())
    }

    /// Estimate type size without lowering to LLVM.
    /// For recursive types and unknown references, use conservative pointer size.
    /// Uses conservative alignment padding to avoid underestimating struct sizes.
    fn estimate_type_size(&self, ty: &Type, type_subst: &HashMap<String, Type>) -> u64 {
        match ty {
            // Zero-size types
            Type::Unit | Type::Prop | Type::Void | Type::Eq(_, _, _) | Type::Error => 0,

            Type::Bool => 1,

            // Pointer-sized types (8 bytes)
            Type::Nat | Type::Mu(_, _) | Type::Ptr(_) | Type::Ref(_) => 8,

            // Closure = 2 pointers
            Type::Arrow(_, _) => 16,

            // String = ptr + length
            Type::String => 16,

            Type::Product(a, b) => {
                let a_size = self.estimate_type_size(a, type_subst);
                let b_size = self.estimate_type_size(b, type_subst);
                let aligned_a = (a_size + 7) & !7;
                aligned_a + b_size
            }
            Type::Sum(a, b) => {
                let payload = self
                    .estimate_type_size(a, type_subst)
                    .max(self.estimate_type_size(b, type_subst));
                let size = 8 + payload;
                (size + 15) & !15
            }
            Type::TyVar(name) => self.estimate_tyvar_size(name, type_subst),
            Type::App(name, _) => {
                if let Some((_, ctors)) = self.adt_types.get(name) {
                    let max_payload = ctors
                        .iter()
                        .map(|ctor| {
                            ctor.fields
                                .iter()
                                .map(|f| self.estimate_type_size(f, type_subst))
                                .map(|s| (s + 7) & !7)
                                .sum::<u64>()
                        })
                        .max()
                        .unwrap_or(0);
                    let max_payload = (max_payload + 7) & !7;
                    return 4 + max_payload;
                }
                8
            }
            Type::Adt(_, _, ctors) => {
                let max_payload = ctors
                    .iter()
                    .map(|(_, payload)| self.estimate_type_size(payload, type_subst))
                    .max()
                    .unwrap_or(0);
                4 + max_payload
            }
            Type::Forall(_, body) => self.estimate_type_size(body, type_subst),
        }
    }

    /// Estimate size for a `TyVar` type.
    fn estimate_tyvar_size(&self, name: &str, type_subst: &HashMap<String, Type>) -> u64 {
        // Strip @-prefix for named types (ADR 13.4.26c §2)
        let name = strip_named_prefix(name);
        // Check substitution first
        if let Some(concrete) = type_subst.get(name) {
            return self.estimate_type_size(concrete, type_subst);
        }
        // Also check instance type_subst
        if let Some(concrete) = self.type_subst.get(name) {
            return self.estimate_type_size(concrete, type_subst);
        }
        // Check if it's a record (fixed size)
        if let Some(fields) = self.record_types.get(name) {
            let mut total = 0;
            for (_field_name, t) in fields {
                let field_size = self.estimate_type_size(t, type_subst);
                total += field_size;
            }
            return total;
        }
        // Check if it's a recursive ADT - treat as pointer since it will be μ-wrapped
        if self.is_recursive_adt(name) {
            return 8;
        }
        // Check if it's a non-recursive ADT - compute actual max payload
        if let Some((params, ctors)) = self.adt_types.get(name) {
            if params.is_empty() {
                // Compute actual max payload size from constructor fields
                let max_payload = ctors
                    .iter()
                    .map(|ctor| {
                        // Sum the sizes of all fields (aligned to 8 for products)
                        ctor.fields
                            .iter()
                            .map(|f| self.estimate_type_size(f, type_subst))
                            .map(|s| (s + 7) & !7) // align each field
                            .sum::<u64>()
                    })
                    .max()
                    .unwrap_or(0);
                // Align total to 8 bytes
                let max_payload = (max_payload + 7) & !7;
                let size = 4 + max_payload; // tag + max payload
                return size;
            }
        }
        8 // Unknown type variable = pointer size
    }

    /// Encode a record type as nested Products.
    ///
    /// `{ f1: T1, f2: T2, f3: T3 }` → `T1 × (T2 × T3)`
    ///
    /// Single-field records are encoded as just the field type.
    pub(super) fn encode_record_type(&self, fields: &[(String, Type)]) -> Type {
        if fields.is_empty() {
            // Empty record = Unit
            Type::Unit
        } else if fields.len() == 1 {
            // Single-field record = just the field type
            fields[0].1.clone()
        } else {
            // Multiple fields: right-nested product
            let mut iter = fields.iter().rev();
            let (_, last_ty) = iter.next().unwrap();
            let mut product = last_ty.clone();
            for (_, ty) in iter {
                product = Type::product(ty.clone(), product);
            }
            product
        }
    }

    /// Encode an ADT as a sum type with substituted type arguments.
    ///
    /// `Option<T>` with [None, Some(T)] → `Unit + T`
    ///
    /// For non-recursive ADTs, we produce a simple sum type.
    /// For recursive ADTs, we would need μ-types but we defer that complexity.
    pub(super) fn encode_adt_type(
        &self,
        constructors: &[CodegenConstructor],
        subst: &HashMap<String, Type>,
    ) -> Type {
        if constructors.is_empty() {
            return Type::Void;
        }

        // Encode each constructor's payload
        let payloads: Vec<Type> = constructors
            .iter()
            .map(|ctor| self.encode_constructor_payload(&ctor.fields, subst))
            .collect();

        // Right-fold into nested sums: [A, B, C] → A + (B + C)
        let mut iter = payloads.into_iter().rev();
        let mut sum = iter.next().unwrap();
        for payload in iter {
            sum = Type::sum(payload, sum);
        }
        sum
    }

    /// Encode a constructor's payload as a product of its fields.
    pub(super) fn encode_constructor_payload(
        &self,
        fields: &[Type],
        subst: &HashMap<String, Type>,
    ) -> Type {
        if fields.is_empty() {
            return Type::Unit;
        }

        // Apply substitution to each field
        let substituted: Vec<Type> = fields
            .iter()
            .map(|ty| self.apply_subst(ty, subst))
            .collect();

        if substituted.len() == 1 {
            return substituted.into_iter().next().unwrap();
        }

        // Multiple fields: left-nested product to match bootstrap encoding
        // [A, B, C] → ((A × B) × C)
        let mut iter = substituted.into_iter();
        let mut product = iter.next().unwrap();
        for ty in iter {
            product = Type::product(product, ty);
        }
        product
    }

    /// Apply a type substitution to a type.
    pub(super) fn apply_subst(&self, ty: &Type, subst: &HashMap<String, Type>) -> Type {
        match ty {
            Type::TyVar(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
            Type::Arrow(l, r) => {
                Type::arrow(self.apply_subst(l, subst), self.apply_subst(r, subst))
            }
            Type::Product(l, r) => {
                Type::product(self.apply_subst(l, subst), self.apply_subst(r, subst))
            }
            Type::Sum(l, r) => Type::sum(self.apply_subst(l, subst), self.apply_subst(r, subst)),
            Type::Forall(v, body) => {
                // Don't substitute bound variables
                if subst.contains_key(v) {
                    ty.clone()
                } else {
                    Type::Forall(v.clone(), Box::new(self.apply_subst(body, subst)))
                }
            }
            Type::Mu(v, body) => {
                // Don't substitute the mu-bound variable
                if subst.contains_key(v) {
                    ty.clone()
                } else {
                    Type::Mu(v.clone(), Box::new(self.apply_subst(body, subst)))
                }
            }
            Type::App(name, args) => {
                let new_args: Vec<Type> = args
                    .iter()
                    .map(|arg| self.apply_subst(arg, subst))
                    .collect();
                Type::App(name.clone(), new_args)
            }
            Type::Eq(ty, t1, t2) => {
                // Equality proofs: only the type needs substitution,
                // t1 and t2 are Terms (not Types), so pass through as-is
                Type::Eq(
                    Box::new(self.apply_subst(ty, subst)),
                    t1.clone(),
                    t2.clone(),
                )
            }
            Type::Ptr(inner) => Type::Ptr(Box::new(self.apply_subst(inner, subst))),
            Type::Ref(inner) => Type::Ref(Box::new(self.apply_subst(inner, subst))),
            // Flat ADT (Phase 2B)
            Type::Adt(name, type_args, variants) => {
                let new_args: Vec<Type> = type_args
                    .iter()
                    .map(|arg| self.apply_subst(arg, subst))
                    .collect();
                let new_variants: Vec<(String, Type)> = variants
                    .iter()
                    .map(|(vname, vty)| (vname.clone(), self.apply_subst(vty, subst)))
                    .collect();
                Type::Adt(name.clone(), new_args, new_variants)
            }
            // Primitive types don't need substitution
            Type::Bool | Type::Nat | Type::Unit | Type::Void | Type::String | Type::Prop => {
                ty.clone()
            }
            Type::Error => Type::Error,
        }
    }
}
