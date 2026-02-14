//! Type Lowering
//!
//! Maps Tungsten Core types to LLVM IR types.
//!
//! # Type Mapping
//!
//! | Tungsten Type | LLVM Type                          |
//! |---------------|------------------------------------|
//! | Bool          | i1                                 |
//! | Nat           | i64                                |
//! | Unit          | {} (empty struct)                  |
//! | Void          | void (never constructed)           |
//! | String        | { i8*, i64 } (ptr + length)        |
//! | τ₁ → τ₂       | { fn(env*, args..)->ret, env* }    |
//! | τ₁ × τ₂       | { τ₁_llvm, τ₂_llvm }               |
//! | τ₁ + τ₂       | { i8 tag, largest(τ₁, τ₂) }        |
//! | ∀α. τ         | (type erased at runtime)           |
//! | Eq τ t₁ t₂    | {} (proof irrelevant)              |
//! | Prop          | {} (proof irrelevant)              |
//! | μα. τ         | i8* (opaque pointer)               |

use inkwell::context::Context;
use inkwell::targets::TargetData;
use inkwell::types::{BasicType, BasicTypeEnum, FunctionType, StructType};
use inkwell::AddressSpace;
use std::collections::HashMap;
use tungsten_core::types::Type;

/// A simplified constructor for codegen purposes.
#[derive(Debug, Clone)]
pub struct CodegenConstructor {
    /// Constructor name (e.g., "Some", "None")
    pub name: String,
    /// Field types (positional)
    pub fields: Vec<Type>,
    /// Index of this constructor in the ADT  
    pub index: usize,
}

/// ADT definition for codegen: params + constructors.
pub type AdtDef = (Vec<String>, Vec<CodegenConstructor>);

/// Manages the mapping from Tungsten types to LLVM types.
pub struct TypeLowering<'ctx> {
    context: &'ctx Context,
    /// Cache of struct types for ADT types (name -> LLVM struct type)
    adt_type_cache: HashMap<String, BasicTypeEnum<'ctx>>,
    /// Record types: name -> fields.
    /// Used to expand `TyVar("RecordName")` to the structural product type.
    record_types: HashMap<String, Vec<(String, Type)>>,
    /// ADT types: name -> (params, constructors).
    /// Used to expand `Type::App("Name", args)` to sum/mu types.
    adt_types: HashMap<String, AdtDef>,
    /// Type variable substitutions for monomorphization.
    /// Used to lower type variables to their concrete types when compiling
    /// type-applied expressions.
    type_subst: HashMap<String, Type>,
    /// Target data for accurate type size calculation with alignment.
    /// When present, uses LLVM's get_store_size for precise sizes.
    target_data: Option<TargetData>,
}

impl<'ctx> TypeLowering<'ctx> {
    /// Create a new type lowering context.
    pub fn new(context: &'ctx Context) -> Self {
        Self {
            context,
            adt_type_cache: HashMap::new(),
            record_types: HashMap::new(),
            adt_types: HashMap::new(),
            type_subst: HashMap::new(),
            target_data: None,
        }
    }

    /// Set target data for accurate type size calculation.
    /// When set, uses LLVM's get_store_size for precise sizes including alignment.
    pub fn set_target_data(&mut self, target_data: TargetData) {
        self.target_data = Some(target_data);
    }

    /// Register record types for expansion during type lowering.
    /// This allows `TyVar("RecordName")` to be lowered to the structural product type.
    pub fn register_record_types(&mut self, records: HashMap<String, Vec<(String, Type)>>) {
        self.record_types = records;
    }

    /// Register ADT types for expansion during type lowering.
    /// This allows `Type::App("Name", args)` to be lowered to sum/mu types.
    pub fn register_adt_types(&mut self, adts: HashMap<String, AdtDef>) {
        self.adt_types = adts;
    }

    /// Push a type substitution for monomorphization.
    /// When lowering type variables, this substitution will be applied first.
    pub fn push_type_subst(&mut self, var: String, ty: Type) {
        self.type_subst.insert(var, ty);
    }

    /// Clear all type substitutions.
    pub fn clear_type_subst(&mut self) {
        self.type_subst.clear();
    }

    /// Restore type substitutions from a saved state.
    pub fn restore_type_subst(&mut self, saved: HashMap<String, Type>) {
        self.type_subst = saved;
    }

    /// Get current type substitutions.
    pub fn type_subst(&self) -> &HashMap<String, Type> {
        &self.type_subst
    }

    /// Check if a type variable name refers to a known concrete type (ADT or record).
    /// Returns true if the name is a registered ADT or record type.
    /// This is used to distinguish concrete types like `Token` (which are represented
    /// as `TyVar("Token")`) from abstract type variables (like `T` in a forall).
    pub fn is_concrete_named_type(&self, name: &str) -> bool {
        self.adt_types.contains_key(name) || self.record_types.contains_key(name)
    }

    /// Apply current type substitution to a type.
    /// Recursively replaces type variables with their bindings.
    pub fn apply_type_subst(&self, ty: &Type) -> Type {
        match ty {
            Type::TyVar(name) => {
                if let Some(concrete_ty) = self.type_subst.get(name) {
                    // Recursively apply in case the substitution contains more type variables
                    self.apply_type_subst(concrete_ty)
                } else {
                    ty.clone()
                }
            }
            Type::Arrow(a, b) => Type::Arrow(
                Box::new(self.apply_type_subst(a)),
                Box::new(self.apply_type_subst(b)),
            ),
            Type::Product(a, b) => Type::Product(
                Box::new(self.apply_type_subst(a)),
                Box::new(self.apply_type_subst(b)),
            ),
            Type::Sum(a, b) => Type::Sum(
                Box::new(self.apply_type_subst(a)),
                Box::new(self.apply_type_subst(b)),
            ),
            Type::App(name, args) => Type::App(
                name.clone(),
                args.iter().map(|t| self.apply_type_subst(t)).collect(),
            ),
            Type::Forall(v, body) => {
                // Don't substitute under a binding of the same name
                if self.type_subst.contains_key(v) {
                    ty.clone()
                } else {
                    Type::Forall(v.clone(), Box::new(self.apply_type_subst(body)))
                }
            }
            Type::Mu(v, body) => {
                if self.type_subst.contains_key(v) {
                    ty.clone()
                } else {
                    Type::Mu(v.clone(), Box::new(self.apply_type_subst(body)))
                }
            }
            Type::Eq(_, _, _)
            | Type::Unit
            | Type::Bool
            | Type::Nat
            | Type::String
            | Type::Prop
            | Type::Void => ty.clone(),
            Type::Ptr(inner) => Type::Ptr(Box::new(self.apply_type_subst(inner))),
            Type::Ref(inner) => Type::Ref(Box::new(self.apply_type_subst(inner))),
            // Flat ADT (Phase 2B)
            Type::Adt(name, type_args, variants) => Type::Adt(
                name.clone(),
                type_args.iter().map(|t| self.apply_type_subst(t)).collect(),
                variants
                    .iter()
                    .map(|(vname, vty)| (vname.clone(), self.apply_type_subst(vty)))
                    .collect(),
            ),
        }
    }

    /// Expand a type without lowering to LLVM.
    /// Expands TyVar (records/ADTs) and App (ADTs) to their structural forms.
    /// Returns None if the type cannot be expanded (not a known record/ADT).
    ///
    /// For n≥3 ADTs, returns Type::Adt (not Sum) for consistency with lower_type.
    pub fn expand_type(&self, ty: &Type) -> Option<Type> {
        match ty {
            Type::TyVar(name) => {
                // First check if this type variable is bound in current substitution
                if let Some(concrete_ty) = self.type_subst.get(name) {
                    return self.expand_type(concrete_ty);
                }
                // Check if this is a record type
                if let Some(fields) = self.record_types.get(name) {
                    return Some(self.encode_record_type(fields));
                }
                // Check if this is a 0-parameter ADT (sum types written as TyVar)
                if let Some((params, constructors)) = self.adt_types.get(name) {
                    if params.is_empty() {
                        // For n≥3 ADTs, return Type::Adt (not Sum) for consistency
                        if constructors.len() >= 3 {
                            let variants: Vec<(String, Type)> = constructors
                                .iter()
                                .map(|c| {
                                    (
                                        c.name.clone(),
                                        self.encode_constructor_payload(&c.fields, &HashMap::new()),
                                    )
                                })
                                .collect();
                            return Some(Type::Adt(name.clone(), vec![], variants));
                        }
                        // n≤2: use existing Sum encoding for backwards compat
                        return Some(self.encode_adt_type(constructors, &HashMap::new()));
                    }
                }
                None
            }
            Type::App(name, args) => {
                // Check if this is an ADT type
                if let Some((params, constructors)) = self.adt_types.get(name) {
                    // Build substitution map, applying current type_subst to resolve type variables
                    let subst: HashMap<String, Type> = params
                        .iter()
                        .zip(args.iter())
                        .map(|(p, a)| (p.clone(), self.apply_type_subst(a)))
                        .collect();

                    // For n≥3 ADTs, return Type::Adt for consistency
                    if constructors.len() >= 3 {
                        let variants: Vec<(String, Type)> = constructors
                            .iter()
                            .map(|c| {
                                (
                                    c.name.clone(),
                                    self.encode_constructor_payload(&c.fields, &subst),
                                )
                            })
                            .collect();
                        let resolved_args: Vec<Type> =
                            args.iter().map(|a| self.apply_type_subst(a)).collect();
                        return Some(Type::Adt(name.clone(), resolved_args, variants));
                    }
                    // n≤2: use existing Sum encoding
                    Some(self.encode_adt_type(constructors, &subst))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Resolve a type to its flat ADT representation (Type::Adt).
    /// Unlike expand_type which returns Sum for compatibility, this returns the
    /// canonical Type::Adt form for use with flat ADT codegen.
    ///
    /// For TyVar("Foo") or App("Foo", args), returns Type::Adt("Foo", args, variants).
    /// For Type::Adt, returns it as-is.
    /// For Type::Mu wrapping an Adt, unwraps and returns the inner Adt (non-recursive form).
    /// Returns None if the type is not an ADT.
    pub fn resolve_to_flat_adt(&self, ty: &Type) -> Option<Type> {
        match ty {
            Type::Adt(_, _, _) => Some(ty.clone()),

            Type::Mu(_, inner) => {
                // For μ X. Adt(...), extract the inner Adt
                self.resolve_to_flat_adt(inner)
            }

            Type::TyVar(name) => {
                // First check type substitutions
                if let Some(concrete_ty) = self.type_subst.get(name) {
                    return self.resolve_to_flat_adt(concrete_ty);
                }

                // Check if this is a 0-parameter ADT
                if let Some((params, constructors)) = self.adt_types.get(name) {
                    if params.is_empty() {
                        let variants: Vec<(String, Type)> = constructors
                            .iter()
                            .map(|ctor| {
                                let payload =
                                    self.encode_constructor_payload(&ctor.fields, &HashMap::new());
                                (ctor.name.clone(), payload)
                            })
                            .collect();
                        return Some(Type::Adt(name.clone(), vec![], variants));
                    }
                }
                None
            }

            Type::App(name, args) => {
                // Check if this is a parameterized ADT
                if let Some((params, constructors)) = self.adt_types.get(name) {
                    let subst: HashMap<String, Type> = params
                        .iter()
                        .zip(args.iter())
                        .map(|(p, a)| (p.clone(), self.apply_type_subst(a)))
                        .collect();

                    let variants: Vec<(String, Type)> = constructors
                        .iter()
                        .map(|ctor| {
                            let payload = self.encode_constructor_payload(&ctor.fields, &subst);
                            (ctor.name.clone(), payload)
                        })
                        .collect();

                    let resolved_args: Vec<Type> =
                        args.iter().map(|a| self.apply_type_subst(a)).collect();

                    return Some(Type::Adt(name.clone(), resolved_args, variants));
                }
                None
            }

            _ => None,
        }
    }

    /// Check if an ADT with the given name is recursive.
    /// An ADT is recursive if any constructor field type contains:
    /// - The Mu variable `α_{name}` (in Mu-encoded form), OR
    /// - The ADT name itself as `TyVar("{name}")` (direct self-reference)
    pub fn is_recursive_adt(&self, name: &str) -> bool {
        let mu_var = format!("α_{}", name);

        if let Some((_, constructors)) = self.adt_types.get(name) {
            for ctor in constructors {
                for field_ty in &ctor.fields {
                    // Check for both α_{name} and {name} since both can represent recursion
                    if Self::type_mentions_var(field_ty, &mu_var)
                        || Self::type_mentions_var(field_ty, name)
                    {
                        return true;
                    }
                }
            }
            false
        } else {
            false
        }
    }

    /// Check if a type mentions a specific type variable (used for recursion detection).
    fn type_mentions_var(ty: &Type, var_name: &str) -> bool {
        match ty {
            Type::TyVar(name) => name == var_name,
            Type::Arrow(t1, t2) | Type::Product(t1, t2) | Type::Sum(t1, t2) => {
                Self::type_mentions_var(t1, var_name) || Self::type_mentions_var(t2, var_name)
            }
            Type::Mu(_, inner) | Type::Forall(_, inner) => Self::type_mentions_var(inner, var_name),
            Type::App(_, type_args) => type_args
                .iter()
                .any(|t| Self::type_mentions_var(t, var_name)),
            Type::Adt(_, _, variants) => variants
                .iter()
                .any(|(_, payload_ty)| Self::type_mentions_var(payload_ty, var_name)),
            Type::Ref(inner) | Type::Ptr(inner) => Self::type_mentions_var(inner, var_name),
            Type::Eq(ty_eq, _, _) => Self::type_mentions_var(ty_eq, var_name),
            Type::Nat | Type::Bool | Type::String | Type::Unit | Type::Prop | Type::Void => false,
        }
    }

    /// Check if a type is uninhabited (has no values).
    ///
    /// Returns true for:
    /// - `Type::Void` (explicitly uninhabited)
    /// - `Type::App("Never", _)` or `Type::TyVar("Never")` (the Never ADT)
    /// - Any ADT with zero constructors
    ///
    /// This is used to emit LLVM `unreachable` after calls to functions that
    /// return uninhabited types (like `exit` which returns `Never`).
    pub fn is_uninhabited_type(&self, ty: &Type) -> bool {
        match ty {
            // Void is explicitly uninhabited
            Type::Void => true,

            // Check for ADT named "Never" (with any number of type args)
            Type::App(name, _) => {
                if name == "Never" {
                    return true;
                }
                // Also check if it's an ADT with zero constructors
                if let Some((_, constructors)) = self.adt_types.get(name) {
                    return constructors.is_empty();
                }
                false
            }

            // Check for 0-parameter ADT named "Never" written as TyVar
            Type::TyVar(name) => {
                if name == "Never" {
                    return true;
                }
                // Also check if it's an ADT with zero constructors
                if let Some((params, constructors)) = self.adt_types.get(name) {
                    if params.is_empty() {
                        return constructors.is_empty();
                    }
                }
                false
            }

            // Check flat ADT form
            Type::Adt(name, _, variants) => name == "Never" || variants.is_empty(),

            _ => false,
        }
    }

    /// Lower a Tungsten type to an LLVM type.
    pub fn lower_type(&mut self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Bool => self.context.bool_type().into(),

            Type::Nat => self.context.i64_type().into(),

            Type::Unit | Type::Prop => {
                // Unit and Prop are empty structs (0 bytes)
                self.context.struct_type(&[], false).into()
            }

            Type::Void => {
                // Void has no values, but we need some representation.
                // Use an empty struct - code that would construct Void is unreachable.
                self.context.struct_type(&[], false).into()
            }

            Type::String => {
                // String = { i8*, i64 } (pointer + length)
                let ptr_type = self.context.ptr_type(AddressSpace::default());
                let len_type = self.context.i64_type();
                self.context
                    .struct_type(&[ptr_type.into(), len_type.into()], false)
                    .into()
            }

            Type::Arrow(param, ret) => {
                // Functions are represented as closures: { fn_ptr, env_ptr }
                // fn_ptr takes (env_ptr, param) -> ret
                self.lower_closure_type(param, ret)
            }

            Type::Product(t1, t2) => {
                // Product = { t1, t2 }
                let llvm_t1 = self.lower_type(t1);
                let llvm_t2 = self.lower_type(t2);
                self.context.struct_type(&[llvm_t1, llvm_t2], false).into()
            }

            Type::Sum(t1, t2) => {
                // Sum = { i8 tag, data }
                // tag: 0 = left, 1 = right
                // data: union of t1 and t2 (we use largest size)
                self.lower_sum_type(t1, t2)
            }

            Type::TyVar(name) => {
                // First check type substitutions for monomorphization
                if let Some(concrete_ty) = self.type_subst.get(name).cloned() {
                    return self.lower_type(&concrete_ty);
                }
                // Check if this is a record type that we should expand
                if let Some(fields) = self.record_types.get(name).cloned() {
                    // Expand record to nested Product (same encoding as elaborator)
                    let expanded = self.encode_record_type(&fields);
                    return self.lower_type(&expanded);
                }
                // Check if this is a 0-parameter ADT
                if let Some((params, constructors)) = self.adt_types.get(name).cloned() {
                    if params.is_empty() {
                        // Use flat { i32 tag, [N x i8] data } representation for ALL non-recursive ADTs
                        // This includes n≤2 constructor ADTs which were previously lowered as Sum types.
                        // Using flat representation avoids infinite recursion through TyVar -> lower_type.
                        if self.is_recursive_adt(name) {
                            eprintln!(
                                "[DEBUG lower_type] TyVar {} is recursive, returning ptr",
                                name
                            );
                            return self.context.ptr_type(AddressSpace::default()).into();
                        }

                        // Check cache first (only for non-recursive ADTs)
                        if let Some(cached) = self.adt_type_cache.get(name) {
                            return *cached;
                        }

                        // Compute flat ADT type using iterative size estimation
                        let tag_type = self.context.i32_type();
                        let variants: Vec<(String, Type)> = constructors
                            .iter()
                            .map(|c| {
                                (
                                    c.name.clone(),
                                    self.encode_constructor_payload(&c.fields, &HashMap::new()),
                                )
                            })
                            .collect();
                        let max_payload =
                            self.compute_max_payload_size_iterative(&variants, &HashMap::new());
                        let payload_size = max_payload.max(1) as u32;
                        let data_type = self.context.i8_type().array_type(payload_size);

                        eprintln!(
                            "[DEBUG lower_type] TyVar {} ({} ctors) -> {{ i32, [{} x i8] }}",
                            name,
                            constructors.len(),
                            payload_size
                        );

                        let result: BasicTypeEnum<'ctx> = self
                            .context
                            .struct_type(&[tag_type.into(), data_type.into()], false)
                            .into();

                        // Cache the result
                        self.adt_type_cache.insert(name.clone(), result);
                        return result;
                    }
                }
                // Type variables should be erased/monomorphized before codegen.
                // For now, treat as opaque pointer.
                self.context.ptr_type(AddressSpace::default()).into()
            }

            Type::Forall(_, body) => {
                // Forall types are erased at runtime - just lower the body
                self.lower_type(body)
            }

            Type::Eq(_, _, _) => {
                // Equality proofs are erased - empty struct
                self.context.struct_type(&[], false).into()
            }

            Type::Mu(alpha, body) => {
                // Recursive types use an opaque pointer.
                // The actual structure is determined when fold/unfold is used.
                self.lower_mu_type(alpha, body)
            }

            // ═══════════════════════════════════════════════════════════════════
            // Phase 3-Prep: Pointers and References
            // ═══════════════════════════════════════════════════════════════════
            Type::Ptr(_inner) => {
                // Pointer type - LLVM's opaque pointer (inner type doesn't affect repr)
                self.context.ptr_type(AddressSpace::default()).into()
            }

            Type::Ref(_inner) => {
                // Ref<T> is implemented as a pointer to heap-allocated T
                // Same representation as Ptr for now
                self.context.ptr_type(AddressSpace::default()).into()
            }

            Type::App(name, args) => {
                // Type::App is a deferred type application from elaboration.
                // Check if this is a known ADT we can expand.
                if let Some((params, constructors)) = self.adt_types.get(name).cloned() {
                    // Build substitution: param -> arg
                    let subst: HashMap<String, Type> = params
                        .iter()
                        .cloned()
                        .zip(args.iter().map(|a| self.apply_type_subst(a)))
                        .collect();

                    // For n≥3 constructors, use flat ADT representation (unless recursive)
                    if constructors.len() >= 3 {
                        // Recursive ADTs are represented as pointers (Mu-wrapped)
                        if self.is_recursive_adt(name) {
                            return self.context.ptr_type(AddressSpace::default()).into();
                        }

                        let tag_type = self.context.i32_type();
                        let variants: Vec<(String, Type)> = constructors
                            .iter()
                            .map(|c| {
                                (
                                    c.name.clone(),
                                    self.encode_constructor_payload(&c.fields, &subst),
                                )
                            })
                            .collect();
                        let max_payload =
                            self.compute_max_payload_size_iterative(&variants, &subst);
                        let payload_size = max_payload.max(1) as u32;
                        let data_type = self.context.i8_type().array_type(payload_size);

                        return self
                            .context
                            .struct_type(&[tag_type.into(), data_type.into()], false)
                            .into();
                    }

                    // n≤2: Use existing Sum representation
                    let expanded = self.encode_adt_type(&constructors, &subst);
                    return self.lower_type(&expanded);
                }
                // Not a known ADT - treat as opaque pointer
                eprintln!(
                    "Warning: Type::App({}) encountered in codegen - should be resolved",
                    name
                );
                self.context.ptr_type(AddressSpace::default()).into()
            }

            // Phase 2B: Flat ADT (ADR 2.2.26)
            // Type::Adt represents a flat enum with direct tag + payload
            Type::Adt(name, type_args, variants) => {
                // Check cache for 0-param ADTs
                if type_args.is_empty() {
                    if let Some(cached) = self.adt_type_cache.get(name) {
                        eprintln!("[DEBUG lower_type] Adt {} using cached struct", name);
                        return *cached;
                    }
                }

                // Lower to { i32 tag, [max_payload x i8] data }
                // Compute actual max payload size for consistency with TyVar/App paths
                let tag_type = self.context.i32_type();
                let max_payload =
                    self.compute_max_payload_size_iterative(variants, &HashMap::new());
                eprintln!(
                    "[DEBUG lower_type] Adt {} creating struct with max_payload={}",
                    name, max_payload
                );
                let payload_size = max_payload.max(1) as u32;
                let data_type = self.context.i8_type().array_type(payload_size);

                let result: BasicTypeEnum<'ctx> = self
                    .context
                    .struct_type(&[tag_type.into(), data_type.into()], false)
                    .into();

                // Cache 0-param ADTs
                if type_args.is_empty() {
                    self.adt_type_cache.insert(name.clone(), result);
                }

                result
            }
        }
    }

    /// Lower a function/closure type.
    ///
    /// Closures are represented as { fn_ptr, env_ptr } where:
    /// - fn_ptr: pointer to function taking (env_ptr, args...) -> ret
    /// - env_ptr: opaque pointer to captured environment
    fn lower_closure_type(&mut self, _param: &Type, _ret: &Type) -> BasicTypeEnum<'ctx> {
        let fn_ptr_type = self.context.ptr_type(AddressSpace::default());
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());

        self.context
            .struct_type(&[fn_ptr_type.into(), env_ptr_type.into()], false)
            .into()
    }

    /// Get the function type for a closure's code pointer.
    ///
    /// The function takes (env_ptr, param) -> ret
    pub fn closure_fn_type(&mut self, param: &Type, ret: &Type) -> FunctionType<'ctx> {
        let env_ptr = self.context.ptr_type(AddressSpace::default());
        let param_ty = self.lower_type(param);
        let ret_ty = self.lower_type(ret);

        ret_ty.fn_type(&[env_ptr.into(), param_ty.into()], false)
    }

    /// Lower a sum type (tagged union).
    fn lower_sum_type(&mut self, t1: &Type, t2: &Type) -> BasicTypeEnum<'ctx> {
        let tag_type = self.context.i32_type();
        let llvm_t1 = self.lower_type(t1);
        let llvm_t2 = self.lower_type(t2);

        // Use the larger of the two types for the data field
        // For simplicity, we'll use an array of i8 large enough to hold either
        let size1 = self.type_size(llvm_t1);
        let size2 = self.type_size(llvm_t2);
        let max_size = size1.max(size2).max(1); // At least 1 byte

        let data_type = self.context.i8_type().array_type(max_size as u32);

        self.context
            .struct_type(&[tag_type.into(), data_type.into()], false)
            .into()
    }

    /// Lower a sum type directly from its Type representation.
    /// Used to avoid infinite recursion when lowering ADTs with 2 constructors.
    fn lower_sum_type_direct(&mut self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Sum(t1, t2) => self.lower_sum_type(t1, t2),
            Type::Unit => self.context.struct_type(&[], false).into(),
            _ => self.lower_type(ty),
        }
    }

    /// Lower a recursive (μ) type.
    fn lower_mu_type(&mut self, _alpha: &str, _body: &Type) -> BasicTypeEnum<'ctx> {
        // Recursive types are represented as opaque pointers.
        // fold wraps a value in a pointer, unfold dereferences it.
        self.context.ptr_type(AddressSpace::default()).into()
    }

    /// Get the size of an LLVM type in bytes.
    /// Uses LLVM TargetData when available for accurate alignment, falls back to conservative estimate.
    pub fn type_size(&self, ty: BasicTypeEnum<'ctx>) -> u64 {
        // Use TargetData for accurate size calculation including alignment padding
        if let Some(ref td) = self.target_data {
            return td.get_store_size(&ty);
        }
        // Fallback with conservative alignment padding
        self.type_size_fallback(ty)
    }

    /// Fallback size calculation with conservative alignment padding.
    /// Used when TargetData is not available.
    fn type_size_fallback(&self, ty: BasicTypeEnum<'ctx>) -> u64 {
        match ty {
            BasicTypeEnum::IntType(t) => (t.get_bit_width() as u64 + 7) / 8,
            BasicTypeEnum::FloatType(_) => 8,
            BasicTypeEnum::PointerType(_) => 8,
            BasicTypeEnum::ArrayType(t) => {
                t.len() as u64 * self.type_size_fallback(t.get_element_type())
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
                t.get_size() as u64 * self.type_size_fallback(t.get_element_type())
            }
        }
    }

    /// Compute max payload size for ADT variants iteratively.
    /// Uses fixed sizes for recursive/pointer types to avoid deep recursion.
    fn compute_max_payload_size_iterative(
        &self,
        variants: &[(String, Type)],
        type_subst: &HashMap<String, Type>,
    ) -> u64 {
        let mut max_size = 0u64;

        for (name, payload_ty) in variants {
            let size = self.estimate_type_size(payload_ty, type_subst);
            eprintln!(
                "[DEBUG codegen] variant {} payload: {:?} -> size {}",
                name, payload_ty, size
            );
            max_size = max_size.max(size);
        }

        eprintln!("[DEBUG codegen] max_payload_size = {}", max_size.max(1));
        max_size.max(1) // Minimum 1 byte for nullary variants
    }

    /// Estimate type size without lowering to LLVM.
    /// For recursive types and unknown references, use conservative pointer size.
    /// Uses conservative alignment padding to avoid underestimating struct sizes.
    fn estimate_type_size(&self, ty: &Type, type_subst: &HashMap<String, Type>) -> u64 {
        match ty {
            Type::Unit | Type::Prop => 0,
            Type::Bool => 1,
            Type::Nat => 8,
            Type::String => 16, // ptr + length
            Type::Product(a, b) => {
                // Align first element to 8 bytes, then add second
                let a_size = self.estimate_type_size(a, type_subst);
                let b_size = self.estimate_type_size(b, type_subst);
                let aligned_a = (a_size + 7) & !7;
                let result = aligned_a + b_size;
                eprintln!(
                    "[DEBUG estimate] Product: a_size={}, b_size={}, aligned_a={}, result={}",
                    a_size, b_size, aligned_a, result
                );
                result
            }
            Type::Sum(a, b) => {
                // Tag (1 byte aligned to 8) + max payload, aligned to 16
                let payload = self
                    .estimate_type_size(a, type_subst)
                    .max(self.estimate_type_size(b, type_subst));
                let size = 8 + payload; // 1 byte tag aligned to 8
                (size + 15) & !15
            }
            Type::Mu(_, _) => 8, // Recursive types are pointers
            Type::Ptr(_) | Type::Ref(_) => 8,
            Type::TyVar(name) => {
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
                    for (field_name, t) in fields.iter() {
                        let field_size = self.estimate_type_size(t, type_subst);
                        eprintln!(
                            "[DEBUG estimate] Record {} field {} type {:?} size {}",
                            name, field_name, t, field_size
                        );
                        total += field_size;
                    }
                    eprintln!("[DEBUG estimate] Record {} total size {}", name, total);
                    return total;
                }
                // Check if it's a recursive ADT - treat as pointer since it will be μ-wrapped
                if self.is_recursive_adt(name) {
                    eprintln!(
                        "[DEBUG estimate] TyVar {} is recursive ADT, treating as pointer (8)",
                        name
                    );
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
                        eprintln!("[DEBUG estimate] TyVar {} is non-recursive ADT, max_payload={}, returning {}", name, max_payload, size);
                        return size;
                    }
                }
                eprintln!("[DEBUG estimate] TyVar {} fallback to 8 (unknown)", name);
                8 // Unknown type variable = pointer size
            }
            Type::App(name, _) => {
                // ADTs with type args: compute actual size if registered
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
                    return 4 + max_payload; // tag + max payload
                }
                8 // Unknown = pointer
            }
            Type::Adt(_, _, ctors) => {
                // Compute actual max payload for inline ADT
                let max_payload = ctors
                    .iter()
                    .map(|(_, payload)| self.estimate_type_size(payload, type_subst))
                    .max()
                    .unwrap_or(0);
                4 + max_payload // tag + max payload
            }
            Type::Arrow(_, _) => 16, // Closure = 2 pointers
            Type::Void => 0,
            Type::Forall(_, body) => self.estimate_type_size(body, type_subst),
            Type::Eq(_, _, _) => 0, // Proof irrelevant
        }
    }

    /// Get the LLVM context.
    pub fn context(&self) -> &'ctx Context {
        self.context
    }

    /// Create the string type.
    pub fn string_type(&self) -> StructType<'ctx> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let len_type = self.context.i64_type();
        self.context
            .struct_type(&[ptr_type.into(), len_type.into()], false)
    }

    /// Get the type used for sum type tags.
    pub fn tag_type(&self) -> inkwell::types::IntType<'ctx> {
        self.context.i8_type()
    }

    /// Encode a record type as nested Products.
    ///
    /// `{ f1: T1, f2: T2, f3: T3 }` → `T1 × (T2 × T3)`
    ///
    /// Single-field records are encoded as just the field type.
    fn encode_record_type(&self, fields: &[(String, Type)]) -> Type {
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
    fn encode_adt_type(
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
    fn encode_constructor_payload(&self, fields: &[Type], subst: &HashMap<String, Type>) -> Type {
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
    fn apply_subst(&self, ty: &Type, subst: &HashMap<String, Type>) -> Type {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lower_bool() {
        let context = Context::create();
        let mut lowering = TypeLowering::new(&context);
        let llvm_ty = lowering.lower_type(&Type::Bool);
        assert!(llvm_ty.is_int_type());
    }

    #[test]
    fn test_lower_nat() {
        let context = Context::create();
        let mut lowering = TypeLowering::new(&context);
        let llvm_ty = lowering.lower_type(&Type::Nat);
        assert!(llvm_ty.is_int_type());
        if let BasicTypeEnum::IntType(int_ty) = llvm_ty {
            assert_eq!(int_ty.get_bit_width(), 64);
        }
    }

    #[test]
    fn test_lower_product() {
        let context = Context::create();
        let mut lowering = TypeLowering::new(&context);
        let ty = Type::product(Type::Bool, Type::Nat);
        let llvm_ty = lowering.lower_type(&ty);
        assert!(llvm_ty.is_struct_type());
    }

    #[test]
    fn test_lower_arrow() {
        let context = Context::create();
        let mut lowering = TypeLowering::new(&context);
        let ty = Type::arrow(Type::Nat, Type::Bool);
        let llvm_ty = lowering.lower_type(&ty);
        // Arrow types are closures (structs with fn_ptr and env_ptr)
        assert!(llvm_ty.is_struct_type());
    }
}
