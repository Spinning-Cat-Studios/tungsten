//! Type application dispatch (`compile_ty_app`).
//!
//! The main entry point for compiling `TyApp` nodes, which dispatches to
//! direct specialization, multi-type-arg peeling, or global monomorphization.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::values::BasicValueEnum;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile type application `t[ty_arg]`.
    ///
    /// Resolution order (ADR 8.5.26g §2.4):
    /// 1. Direct specialization: `(TyAbs var body)[ty_arg]` → substitute and compile in-place
    /// 2. Multi-type-arg: `TyApp(TyApp(Global(name), A), B)` → peel nested `TyApp` to collect
    ///    all type args, then monomorphize with all args at once
    /// 3. Global specialization: `global_fn[ty_arg]` → look up pre-seeded mono instance
    ///    (when `mono_map_active`), or generate ad-hoc specialized copy (legacy path)
    /// 4. Fallback: compile the inner term with type erasure
    ///
    /// When the single-owner mono pipeline is active, step 3 consults
    /// `MonomorphState.instances` (pre-seeded via `register_mono_instance`) and
    /// rejects any instance not found there. This ensures every `TyApp(Global(…), …)`
    /// resolves to the centrally-assigned symbol from the ownership map.
    pub(crate) fn compile_ty_app(
        &mut self,
        t: &Term,
        ty_arg: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        match t {
            Term::TyAbs(var, body) => {
                // Direct specialization
                self.types.push_type_subst(var.clone(), ty_arg.clone());
                let result = self.compile_term(body);
                self.types.clear_type_subst(); // Simple approach: clear all
                result
            }
            Term::TyApp(_, _) => self.compile_ty_app_multi(t, ty_arg),
            Term::Global(name) => self.compile_ty_app_global(name, ty_arg),
            _ => {
                // Fall back to erasure for complex expressions
                self.compile_term(t)
            }
        }
    }

    /// Handle multi-type-arg TyApp: peel nested TyApp layers to find the base.
    /// e.g., TyApp(TyApp(Global("pmap"), A), B) → ("pmap", [A, B])
    fn compile_ty_app_multi(
        &mut self,
        t: &Term,
        ty_arg: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let mut type_args = vec![ty_arg.clone()];
        let mut current = t;
        while let Term::TyApp(inner, arg) = current {
            type_args.push(arg.clone());
            current = inner.as_ref();
        }
        type_args.reverse(); // collected in reverse order

        match current {
            Term::TyAbs(var, body) => {
                // Direct multi-arg specialization: peel TyAbs layers
                // and apply each type arg in order.
                self.types
                    .push_type_subst(var.clone(), type_args[0].clone());
                let result = self.compile_term_with_multi_type_subst(body, &type_args[1..]);
                self.types.clear_type_subst();
                result
            }
            Term::Global(name) => {
                // Multi-arg global monomorphization.
                // Apply current type substitution to all type arguments.
                let resolved_args: Vec<Type> = type_args
                    .iter()
                    .map(|a| {
                        let resolved = self.types.apply_type_subst(a);
                        self.strip_at_prefix_tyvars(&resolved)
                    })
                    .collect();

                // Check for unresolved type variables
                if resolved_args
                    .iter()
                    .any(|a| self.has_mono_blocking_tyvar(a))
                {
                    let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                    let closure_type = self
                        .context
                        .struct_type(&[ptr_type.into(), ptr_type.into()], false);
                    return Ok(closure_type.const_zero().into());
                }

                // Look up or compile the multi-arg monomorphized version
                if let Some(ty) = self.defs.def_types.get(name).cloned() {
                    if matches!(ty, Type::Forall(_, _)) {
                        if let Some(specialized) =
                            self.compile_monomorphized_multi(name, &resolved_args)?
                        {
                            return Ok(specialized);
                        }
                    }
                }
                // Fall back to erasure
                self.compile_term(t)
            }
            _ => {
                // Non-Global, non-TyAbs base — fall back to erasure
                self.compile_term(t)
            }
        }
    }

    /// Handle single-type-arg TyApp on a Global reference.
    fn compile_ty_app_global(
        &mut self,
        name: &str,
        ty_arg: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Apply current type substitution to the type argument
        // This handles cases like calling list_reverse_acc<T> inside list_reverse<String>
        // where T should be substituted with String
        let resolved_ty_arg = self.types.apply_type_subst(ty_arg);

        // Strip @-prefixes from Phase 1c TyVar references (e.g., @Token → Token).
        // These are concrete named types, not abstract type variables.
        let resolved_ty_arg = self.strip_at_prefix_tyvars(&resolved_ty_arg);

        // Check for unresolved type variables that can't be monomorphized.
        // TyVars appear in two cases:
        //   1. Abstract type params (T) — can't mono
        //   2. Concrete named types (Token, List) encoded as TyVar — CAN mono
        // @-prefixed TyVars (@Token) are Phase 1c artifacts stripped above.
        // For top-level TyVars, use is_concrete_named_type to distinguish.
        // For compound types, recursively check for non-concrete TyVars.
        if self.has_mono_blocking_tyvar(&resolved_ty_arg) {
            // Inside a polymorphic body with unresolved type vars.
            // Return a zeroed closure placeholder — it will never execute at
            // runtime because the outer TyAbs will be monomorphized first.
            let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
            let closure_type = self
                .context
                .struct_type(&[ptr_type.into(), ptr_type.into()], false);
            return Ok(closure_type.const_zero().into());
        }

        // Check if we need monomorphization
        if let Some(ty) = self.defs.def_types.get(name).cloned() {
            if matches!(ty, Type::Forall(_, _)) {
                // This is a polymorphic function call
                // Try to get or create monomorphized version
                if let Some(specialized) = self.compile_monomorphized(name, &resolved_ty_arg)? {
                    return Ok(specialized);
                }
            }
        }
        // Fall back to erasure
        self.compile_term(&Term::Global(name.to_string()))
    }
}
