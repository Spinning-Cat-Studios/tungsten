//! Type lowering — maps Tungsten types to LLVM IR types.

use super::strip_named_prefix;
use super::TypeLowering;
use inkwell::types::{BasicType, BasicTypeEnum, FunctionType};
use inkwell::AddressSpace;
use std::collections::HashMap;
use tungsten_core::types::Type;

impl<'ctx> TypeLowering<'ctx> {
    /// Lower a Tungsten type to an LLVM type.
    pub fn lower_type(&mut self, ty: &Type) -> BasicTypeEnum<'ctx> {
        self.lower_type_depth += 1;
        if self.lower_type_depth > 200 {
            let def_name = self.current_def_name.as_deref().unwrap_or("<unknown>");
            eprintln!(
                "[codegen error] lower_type recursion depth {} exceeded limit \
                 while compiling '{}', type: {:?}",
                self.lower_type_depth, def_name, ty
            );
            self.lower_type_depth -= 1;
            return self.context.ptr_type(AddressSpace::default()).into();
        }
        let result = self.lower_type_inner(ty);
        self.lower_type_depth -= 1;
        result
    }

    fn lower_type_inner(&mut self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Bool => self.context.bool_type().into(),
            Type::Nat => self.context.i64_type().into(),

            // Zero-size types — empty structs
            Type::Unit | Type::Prop | Type::Void | Type::Eq(_, _, _) => {
                self.context.struct_type(&[], false).into()
            }

            Type::String => {
                let ptr_type = self.context.ptr_type(AddressSpace::default());
                let len_type = self.context.i64_type();
                self.context
                    .struct_type(&[ptr_type.into(), len_type.into()], false)
                    .into()
            }

            Type::Arrow(param, ret) => self.lower_closure_type(param, ret),

            Type::Product(t1, t2) => {
                let llvm_t1 = self.lower_type(t1);
                let llvm_t2 = self.lower_type(t2);
                self.context.struct_type(&[llvm_t1, llvm_t2], false).into()
            }

            Type::Sum(t1, t2) => self.lower_sum_type(t1, t2),
            Type::TyVar(name) => self.lower_tyvar(name),
            Type::Forall(_, body) => self.lower_type(body),
            Type::Mu(alpha, body) => self.lower_mu_type(alpha, body),

            // Pointer types — opaque pointer
            Type::Ptr(_) | Type::Ref(_) => self.context.ptr_type(AddressSpace::default()).into(),

            Type::App(name, args) => self.lower_app(name, args),
            Type::Adt(name, type_args, variants) => self.lower_adt(name, type_args, variants),

            Type::Error => {
                eprintln!("Warning: Type::Error reached codegen - using empty struct");
                self.context.struct_type(&[], false).into()
            }
        }
    }

    /// Lower a `TyVar` type.
    fn lower_tyvar(&mut self, name: &str) -> BasicTypeEnum<'ctx> {
        // Strip @-prefix for named types (ADR 13.4.26c §2)
        let name = strip_named_prefix(name);
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
                return self.lower_nullary_adt(name, &constructors);
            }
        }
        self.report_tyvar_fallthrough(name)
    }

    /// Lower a 0-parameter ADT type to its LLVM representation.
    fn lower_nullary_adt(
        &mut self,
        name: &str,
        constructors: &[super::CodegenConstructor],
    ) -> BasicTypeEnum<'ctx> {
        // Use flat { i32 tag, [N x i8] data } representation for ALL non-recursive ADTs
        // This includes n≤2 constructor ADTs which were previously lowered as Sum types.
        // Using flat representation avoids infinite recursion through TyVar -> lower_type.
        if self.is_recursive_adt(name) {
            return self.context.ptr_type(AddressSpace::default()).into();
        }

        // Single-constructor ADTs: match the elaborator's encoding policy.
        // The elaborator encodes 1-constructor ADTs as bare payload (no Sum/tag wrapper),
        // and the injection codegen returns the value directly without AdtConstruct.
        // We must lower the type consistently: bare payload, not { i32, data }.
        if constructors.len() == 1 {
            let payload_ty =
                self.encode_constructor_payload(&constructors[0].fields, &HashMap::new());
            return self.lower_type(&payload_ty);
        }

        // Check cache first (only for non-recursive ADTs)
        if let Some(cached) = self.adt_type_cache.get(name) {
            return *cached;
        }

        // W4: Compute flat ADT type using largest variant's concrete LLVM type
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
        let data_type = self.compute_largest_payload_llvm_type(&variants);

        let result: BasicTypeEnum<'ctx> = self
            .context
            .struct_type(&[tag_type.into(), data_type], false)
            .into();

        // Cache the result
        self.adt_type_cache.insert(name.to_string(), result);
        result
    }

    /// Report a `TyVar` that reached codegen without being resolved.
    fn report_tyvar_fallthrough(&mut self, name: &str) -> BasicTypeEnum<'ctx> {
        self.tyvar_fallthrough_count += 1;
        if self.tyvar_fallthrough_count <= 10 {
            let def_name = self.current_def_name.as_deref().unwrap_or("<unknown>");
            eprintln!(
                "[codegen warning] TyVar({:?}) reached lower_type \
                 (fallthrough #{})\n  compiling: '{}'",
                name, self.tyvar_fallthrough_count, def_name
            );
            // W3.1 Tool 4: --codegen-backtrace (ADR 13.4.26b)
            if self.codegen_backtrace {
                let bt = std::backtrace::Backtrace::force_capture();
                let bt_str = bt.to_string();
                // Filter to tungsten frames only
                let filtered: Vec<&str> = bt_str
                    .lines()
                    .filter(|line| {
                        line.contains("tungsten_codegen")
                            || line.contains("tungsten_bootstrap")
                            || line.contains("tungsten_core")
                    })
                    .collect();
                if !filtered.is_empty() {
                    eprintln!("  backtrace:");
                    for frame in &filtered {
                        eprintln!("    {}", frame.trim());
                    }
                }
            }
        }
        debug_assert!(
            false,
            "TyVar({name:?}) should not reach codegen — \
             elaborator failed to resolve type variable"
        );
        // Fallback: opaque pointer (preserves current behavior in release)
        self.context.ptr_type(AddressSpace::default()).into()
    }

    /// Lower a `Type::App` (deferred type application).
    fn lower_app(&mut self, name: &str, args: &[Type]) -> BasicTypeEnum<'ctx> {
        // Type::App is a deferred type application from elaboration.
        // Check if this is a known ADT we can expand.
        if let Some((params, constructors)) = self.adt_types.get(name).cloned() {
            // Recursive ADTs are always represented as opaque pointers (Mu-wrapped),
            // regardless of constructor count. The Core IR wraps values with fold
            // and extracts with unfold, which expects pointer representation.
            if self.is_recursive_adt(name) {
                return self.context.ptr_type(AddressSpace::default()).into();
            }

            // Mark as in-progress before expanding (for cycle detection in
            // mutually recursive non-Mu types, if any).
            self.lowering_in_progress.insert(name.to_string());

            // Build substitution: param -> arg
            let subst: HashMap<String, Type> = params
                .iter()
                .cloned()
                .zip(args.iter().map(|a| self.apply_type_subst(a)))
                .collect();

            let result = if constructors.len() >= 3 {
                // W4: Use largest variant's concrete LLVM type instead of opaque [N x i8]
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
                let data_type = self.compute_largest_payload_llvm_type(&variants);

                self.context
                    .struct_type(&[tag_type.into(), data_type], false)
                    .into()
            } else {
                // n≤2: Use existing Sum representation
                let expanded = self.encode_adt_type(&constructors, &subst);
                self.lower_type(&expanded)
            };

            self.lowering_in_progress.remove(name);
            return result;
        }
        // Not a known ADT - treat as opaque pointer
        eprintln!("Warning: Type::App({name}) encountered in codegen - should be resolved");
        self.context.ptr_type(AddressSpace::default()).into()
    }

    /// Lower a `Type::Adt` (flat enum with direct tag + payload).
    fn lower_adt(
        &mut self,
        name: &str,
        type_args: &[Type],
        variants: &[(String, Type)],
    ) -> BasicTypeEnum<'ctx> {
        // Check cache for 0-param ADTs
        if type_args.is_empty() {
            if let Some(cached) = self.adt_type_cache.get(name) {
                return *cached;
            }
        }

        // W5: Lower to { i32 tag, [N x i8] } with opaque byte array for data.
        //
        // We use a byte array instead of the typed largest variant to avoid
        // ABI decomposition issues on ARM64 (and potentially other platforms).
        // When LLVM passes a struct in registers, it decomposes based on field
        // types. If variant A has an i32 at offset X but variant B has a ptr at
        // the same offset, the upper bytes of the pointer get truncated.
        // Using [N x i8] ensures uniform byte-level decomposition.
        let tag_type = self.context.i32_type();
        let largest_type = self.compute_largest_payload_llvm_type(variants);
        let data_size = self.type_size(largest_type);
        let data_type = self.context.i8_type().array_type(data_size as u32);

        let result: BasicTypeEnum<'ctx> = self
            .context
            .struct_type(&[tag_type.into(), data_type.into()], false)
            .into();

        // Cache 0-param ADTs
        if type_args.is_empty() {
            self.adt_type_cache.insert(name.to_string(), result);
        }

        result
    }

    /// Lower a function/closure type.
    ///
    /// Closures are represented as { `fn_ptr`, `env_ptr` } where:
    /// - `fn_ptr`: pointer to function taking (`env_ptr`, args...) -> ret
    /// - `env_ptr`: opaque pointer to captured environment
    fn lower_closure_type(&mut self, _param: &Type, _ret: &Type) -> BasicTypeEnum<'ctx> {
        let fn_ptr_type = self.context.ptr_type(AddressSpace::default());
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());

        self.context
            .struct_type(&[fn_ptr_type.into(), env_ptr_type.into()], false)
            .into()
    }

    /// Get the function type for a closure's code pointer.
    ///
    /// The function takes (`env_ptr`, param) -> ret
    pub fn closure_fn_type(&mut self, param: &Type, ret: &Type) -> FunctionType<'ctx> {
        let env_ptr = self.context.ptr_type(AddressSpace::default());
        let param_ty = self.lower_type(param);
        let ret_ty = self.lower_type(ret);

        ret_ty.fn_type(&[env_ptr.into(), param_ty.into()], false)
    }

    /// Lower a sum type (tagged union).
    ///
    /// W5 (ADR 11.4.26c): Uses opaque `[N x i8]` for the data field, matching
    /// the `lower_adt` strategy. W4 originally used the larger variant's
    /// concrete LLVM type, but W5 demonstrated this is ABI-unsafe: when LLVM
    /// decomposes structs for register passing, typed fields cause
    /// variant-dependent register sizing that can truncate pointers.
    /// Using `[N x i8]` ensures uniform byte-level decomposition.
    fn lower_sum_type(&mut self, t1: &Type, t2: &Type) -> BasicTypeEnum<'ctx> {
        let tag_type = self.context.i32_type();
        let llvm_t1 = self.lower_type(t1);
        let llvm_t2 = self.lower_type(t2);

        let size1 = self.type_size(llvm_t1);
        let size2 = self.type_size(llvm_t2);
        let data_size = std::cmp::max(size1, size2);
        let data_type = self.context.i8_type().array_type(data_size as u32);

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
}
