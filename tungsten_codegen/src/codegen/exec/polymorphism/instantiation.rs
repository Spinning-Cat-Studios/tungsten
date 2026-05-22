//! Monomorphized function instantiation.
//!
//! Handles compilation of specialized copies of polymorphic functions:
//! - Single-type-arg instantiation (`compile_monomorphized`)
//! - Multi-type-arg instantiation (`compile_monomorphized_multi`)
//! - Named instantiation for the single-owner mono pipeline (`compile_monomorphized_named`)

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::BasicValueEnum;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Try to compile a monomorphized version of a polymorphic function.
    ///
    /// Returns Some(value) if we have the original term and can specialize it,
    /// or None if we should fall back to erasure.
    pub(crate) fn compile_monomorphized(
        &mut self,
        name: &str,
        ty_arg: &Type,
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodeGenError> {
        let ty_key = format!("{ty_arg:?}");
        let mono_key = (name.to_string(), ty_key.clone());

        // Check if we already have this monomorphized version
        if let Some(specialized_name) = self.monomorph.instances.get(&mono_key) {
            if let Some(func) = self.module.get_function(specialized_name) {
                return Ok(Some(self.wrap_function_as_closure(func)?));
            }
        }

        // Extract the polymorphic body — returns None if not available / not polymorphic
        let (type_var, inner_term, inner_type) = match self.extract_poly_body(name) {
            Some(parts) => parts,
            None => return Ok(None),
        };

        // Guard: when the single-owner mono pipeline is active (ADR 8.5.26g §2.1.1),
        // all instances must come from the pre-seeded ownership map.
        if self.monomorph.mono_map_active {
            let unit = self.naming.module_prefix.as_deref().unwrap_or("<unknown>");
            return Err(CodeGenError::LlvmError(format!(
                "ICE: ad-hoc monomorphization of '{name}<{ty_arg:?}>' rejected in unit '{unit}' — \
                 mono_map is active but no pre-seeded instance found. \
                 Canonical type args: {ty_key:?}. \
                 Run with --trace-mono for discovery/ownership details."
            )));
        }

        // Generate specialized name (include module prefix to avoid cross-module collisions)
        let specialized_name = if let Some(ref prefix) = self.naming.module_prefix {
            format!("{}__mono_{}_{}", name, prefix, self.naming.counter)
        } else {
            format!("{}__mono_{}", name, self.naming.counter)
        };
        self.naming.counter += 1;

        self.monomorph
            .instances
            .insert(mono_key.clone(), specialized_name.clone());
        self.monomorph.in_progress.insert(mono_key.clone());

        let specialized_ty = inner_type.substitute(&type_var, ty_arg);
        self.declare_def(&specialized_name, &specialized_ty)?;

        // Compile the specialized function body under saved/restored state
        let result = self.compile_with_type_subst(
            &type_var,
            ty_arg,
            &specialized_name,
            &inner_term,
            &specialized_ty,
        );

        self.monomorph.in_progress.remove(&mono_key);
        result?;

        if let Some(func) = self.module.get_function(&specialized_name) {
            Ok(Some(self.wrap_function_as_closure(func)?))
        } else {
            Err(CodeGenError::LlvmError(format!(
                "Failed to get monomorphized function {specialized_name}"
            )))
        }
    }

    /// Try to compile a monomorphized version of a multi-type-param polymorphic function.
    ///
    /// Like `compile_monomorphized` but handles `[A, B, ...]` type args.
    pub(crate) fn compile_monomorphized_multi(
        &mut self,
        name: &str,
        type_args: &[Type],
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodeGenError> {
        let ty_key = Self::format_type_args_key(type_args);
        let mono_key = (name.to_string(), ty_key.clone());

        // Check if we already have this monomorphized version
        if let Some(specialized_name) = self.monomorph.instances.get(&mono_key) {
            if let Some(func) = self.module.get_function(specialized_name) {
                return Ok(Some(self.wrap_function_as_closure(func)?));
            }
        }

        // Extract ALL polymorphic layers
        let (type_vars, inner_term, inner_type) = match self.extract_poly_body_multi(name) {
            Some(parts) => parts,
            None => return Ok(None),
        };

        // Guard: when mono_map is active, all instances must be pre-seeded
        if self.monomorph.mono_map_active {
            let unit = self.naming.module_prefix.as_deref().unwrap_or("<unknown>");
            return Err(CodeGenError::LlvmError(format!(
                "ICE: ad-hoc monomorphization of '{name}<{ty_key}>' rejected in unit '{unit}' — \
                 mono_map is active but no pre-seeded instance found. \
                 Run with --trace-mono for discovery/ownership details."
            )));
        }

        // Generate specialized name
        let specialized_name = if let Some(ref prefix) = self.naming.module_prefix {
            format!("{}__mono_{}_{}", name, prefix, self.naming.counter)
        } else {
            format!("{}__mono_{}", name, self.naming.counter)
        };
        self.naming.counter += 1;

        self.monomorph
            .instances
            .insert(mono_key.clone(), specialized_name.clone());
        self.monomorph.in_progress.insert(mono_key.clone());

        // Substitute all type args
        let mut specialized_ty = inner_type.clone();
        for (var, arg) in type_vars.iter().zip(type_args.iter()) {
            specialized_ty = specialized_ty.substitute(var, arg);
        }
        self.declare_def(&specialized_name, &specialized_ty)?;

        let result = self.compile_with_multi_type_subst(
            &type_vars,
            type_args,
            &specialized_name,
            &inner_term,
            &specialized_ty,
        );

        self.monomorph.in_progress.remove(&mono_key);
        result?;

        if let Some(func) = self.module.get_function(&specialized_name) {
            Ok(Some(self.wrap_function_as_closure(func)?))
        } else {
            Err(CodeGenError::LlvmError(format!(
                "Failed to get monomorphized function {specialized_name}"
            )))
        }
    }

    /// Compile a monomorphized instance with a specific target symbol name.
    ///
    /// Used by the single-owner mono pipeline (ADR 8.5.26g): the owner unit
    /// compiles each assigned instance under the mangled symbol from the
    /// ownership map, instead of generating a fresh per-unit name.
    ///
    /// `global_name` is the original polymorphic definition name (e.g. `"map"`).
    /// `type_args` is the concrete type argument(s) (e.g. `[Type::TyVar("Nat")]`).
    /// `target_symbol` is the mangled symbol from the mono ownership map.
    ///
    /// Returns `Ok(())` on success. The function is emitted as `define @target_symbol(...)`.
    pub fn compile_monomorphized_named(
        &mut self,
        global_name: &str,
        type_args: &[Type],
        target_symbol: &str,
    ) -> Result<(), CodeGenError> {
        // Check if already compiled (idempotent)
        if self.module.get_function(target_symbol).is_some() {
            return Ok(());
        }

        let (type_vars, inner_term, inner_type) = self.extract_poly_body_multi(global_name)
            .ok_or_else(|| CodeGenError::LlvmError(format!(
                "mono owner: '{global_name}' has no polymorphic body (TyAbs/Forall) for '{target_symbol}'"
            )))?;

        if type_args.len() > type_vars.len() {
            return Err(CodeGenError::LlvmError(format!(
                "mono owner: '{}' has {} type params but {} type args for '{}'",
                global_name,
                type_vars.len(),
                type_args.len(),
                target_symbol
            )));
        }

        // Substitute all type args into the inner type
        let mut specialized_ty = inner_type.clone();
        for (var, ty_arg) in type_vars.iter().zip(type_args.iter()) {
            specialized_ty = specialized_ty.substitute(var, ty_arg);
        }

        // Register in MonomorphState so call-site resolution finds it
        let ty_key = Self::format_type_args_key(type_args);
        let mono_key = (global_name.to_string(), ty_key);
        self.monomorph
            .instances
            .insert(mono_key.clone(), target_symbol.to_string());

        self.declare_def(target_symbol, &specialized_ty)?;

        let result = self.compile_with_multi_type_subst(
            &type_vars,
            type_args,
            target_symbol,
            &inner_term,
            &specialized_ty,
        );

        self.monomorph.in_progress.remove(&mono_key);
        result?;
        Ok(())
    }

    /// Format type args as a mono key string.
    fn format_type_args_key(type_args: &[Type]) -> String {
        if type_args.len() == 1 {
            format!("{:?}", type_args[0])
        } else {
            let parts: Vec<String> = type_args.iter().map(|t| format!("{t:?}")).collect();
            parts.join(", ")
        }
    }
}
