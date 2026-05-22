//! Polymorphism compilation (monomorphization).
//!
//! Handles compilation of:
//! - `TyAbs` (type abstraction / forall intro)
//! - `TyApp` (type application / forall elim)
//! - Monomorphized function specialization
//!
//! Split into submodules:
//! - `dispatch`: `compile_ty_app` — the main TyApp dispatch logic
//! - `instantiation`: `compile_monomorphized*` — specialized copy generation

mod dispatch;
mod instantiation;

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Extract the polymorphic body (`type_var`, `inner_term`, `inner_type`) from
    /// a named definition, or None if the definition is unavailable or not
    /// a TyAbs/Forall pair.
    fn extract_poly_body(&self, name: &str) -> Option<(String, Term, Type)> {
        let original_term = self.defs.term_defs.get(name)?;
        let original_ty = self.defs.def_types.get(name)?;
        match (original_term, original_ty) {
            (Term::TyAbs(var, body), Type::Forall(_, inner_ty)) => Some((
                var.clone(),
                body.as_ref().clone(),
                inner_ty.as_ref().clone(),
            )),
            _ => None,
        }
    }

    /// Extract ALL nested polymorphic layers from a definition.
    ///
    /// For `TyAbs(A, TyAbs(B, body))` / `Forall(A, Forall(B, inner))`,
    /// returns `([A, B], body, inner)` where `inner` is the fully unwrapped type.
    fn extract_poly_body_multi(&self, name: &str) -> Option<(Vec<String>, Term, Type)> {
        let original_term = self.defs.term_defs.get(name)?;
        let original_ty = self.defs.def_types.get(name)?;

        let mut type_vars = Vec::new();
        let mut current_term = original_term.clone();
        let mut current_type = original_ty.clone();

        while let (Term::TyAbs(var, body), Type::Forall(_, inner_ty)) =
            (&current_term, &current_type)
        {
            type_vars.push(var.clone());
            current_term = body.as_ref().clone();
            current_type = inner_ty.as_ref().clone();
        }

        if type_vars.is_empty() {
            None
        } else {
            Some((type_vars, current_term, current_type))
        }
    }

    /// Compile a definition body under a type substitution, saving and
    /// restoring all compiler state around the compilation.
    fn compile_with_type_subst(
        &mut self,
        type_var: &str,
        ty_arg: &Type,
        def_name: &str,
        body: &Term,
        body_ty: &Type,
    ) -> Result<(), CodeGenError> {
        let saved_block = self.builder.get_insert_block();
        let saved_env = self.compilation.env.clone();
        let saved_current_fn = self.compilation.current_fn;
        let saved_type_subst = self.types.type_subst().clone();

        self.types
            .push_type_subst(type_var.to_string(), ty_arg.clone());
        let result = self.compile_def(def_name, body, body_ty);

        self.types.restore_type_subst(saved_type_subst);
        self.compilation.env = saved_env;
        self.compilation.current_fn = saved_current_fn;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }

        result.map(|_| ())
    }

    /// Compile a definition body under multiple type substitutions.
    ///
    /// Pushes all (`type_var`, `ty_arg`) pairs, compiles the body, then restores state.
    fn compile_with_multi_type_subst(
        &mut self,
        type_vars: &[String],
        type_args: &[Type],
        def_name: &str,
        body: &Term,
        body_ty: &Type,
    ) -> Result<(), CodeGenError> {
        let saved_block = self.builder.get_insert_block();
        let saved_env = self.compilation.env.clone();
        let saved_current_fn = self.compilation.current_fn;
        let saved_type_subst = self.types.type_subst().clone();

        for (var, arg) in type_vars.iter().zip(type_args.iter()) {
            self.types.push_type_subst(var.clone(), arg.clone());
        }
        let result = self.compile_def(def_name, body, body_ty);

        self.types.restore_type_subst(saved_type_subst);
        self.compilation.env = saved_env;
        self.compilation.current_fn = saved_current_fn;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }

        result.map(|_| ())
    }

    /// Compile a term with multiple type substitutions applied inline.
    ///
    /// Used for direct specialization of nested `TyAbs` (e.g., TyAbs(B, body))
    /// when the outer layers have already been peeled.
    fn compile_term_with_multi_type_subst(
        &mut self,
        term: &Term,
        remaining_type_args: &[Type],
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        if remaining_type_args.is_empty() {
            return self.compile_term(term);
        }
        // Peel one TyAbs layer and apply the next type arg
        if let Term::TyAbs(var, body) = term {
            self.types
                .push_type_subst(var.clone(), remaining_type_args[0].clone());
            self.compile_term_with_multi_type_subst(body, &remaining_type_args[1..])
        } else {
            // No more TyAbs to peel — just compile with remaining args unused
            self.compile_term(term)
        }
    }

    /// Recursively check whether a type contains any `TyVar` that blocks
    /// monomorphization.
    ///
    /// Delegates to the shared `Type::has_mono_blocking_tyvar` predicate
    /// (defined in `tungsten_core::types::tyvar_ops`), using the cached
    /// concrete type name set from `TypeLowering`. See ADR 13.5.26a.
    fn has_mono_blocking_tyvar(&self, ty: &Type) -> bool {
        ty.has_mono_blocking_tyvar(&self.types.concrete_type_names)
    }

    /// Strip `@` prefixes from `TyVars` in a type tree.
    ///
    /// `@`-prefixed `TyVars` are Phase 1c artifacts referencing concrete named types
    /// (e.g., `@Token` → `Token`). Stripping ensures mono key matching between
    /// the discovery phase (which strips) and codegen (which receives raw types).
    ///
    /// Note: as of ADR 10.5.26d P7, `@`-prefixed `TyVars` are stripped at the
    /// elaboration→codegen boundary (`CoreDef::strip_at_prefixes`). This method
    /// is retained as a defense-in-depth safety net — it should be a no-op on
    /// well-formed input.
    fn strip_at_prefix_tyvars(&self, ty: &Type) -> Type {
        ty.strip_tyvar_at_prefix()
    }

    /// Wrap a function value as a closure with null environment.
    fn wrap_function_as_closure(
        &self,
        func: inkwell::values::FunctionValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let env_ptr_type = self.context.ptr_type(AddressSpace::default());
        let closure_type = self
            .context
            .struct_type(&[env_ptr_type.into(), env_ptr_type.into()], false);
        let null_env = env_ptr_type.const_null();
        let mut closure = closure_type.const_zero();
        closure = self
            .builder
            .build_insert_value(
                closure,
                func.as_global_value().as_pointer_value(),
                0,
                "fn_ptr",
            )
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();
        closure = self
            .builder
            .build_insert_value(closure, null_env, 1, "null_env")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
            .into_struct_value();
        Ok(closure.into())
    }
}

#[cfg(test)]
mod tests;
