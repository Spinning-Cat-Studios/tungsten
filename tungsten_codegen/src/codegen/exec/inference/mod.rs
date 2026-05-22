//! Type inference for code generation.
//!
//! Requires type annotations for complex cases.

// Tests: tests.rs

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use std::collections::HashMap;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl CodeGen<'_> {
    /// Infer the type of a term (simplified - requires type annotations).
    pub(crate) fn infer_term_type(&self, term: &Term) -> Result<Type, CodeGenError> {
        self.infer_term_type_with_ctx(term, &HashMap::new())
    }

    /// Infer type with additional local bindings (for Let, Lambda, Case).
    pub(crate) fn infer_term_type_with_ctx(
        &self,
        term: &Term,
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        // Constant-type returns: terms whose type is always the same
        if let Some(ty) = infer_constant_type(term) {
            return Ok(ty);
        }
        // Annotation-passthrough: terms that carry their own type
        if let Some(ty) = infer_annotated_type(term) {
            return Ok(ty.clone());
        }
        match term {
            // Variables
            Term::Var(x) => self.resolve_var_type(x, local_ctx),
            Term::Global(name) => self.infer_global_type(name),
            Term::ExternCall(symbol, _) => self.infer_extern_call_type(symbol),

            // Products
            Term::Pair(t1, t2) => self.infer_pair_type(t1, t2, local_ctx),
            Term::Fst(t) => self.infer_product_projection(t, local_ctx, true),
            Term::Snd(t) => self.infer_product_projection(t, local_ctx, false),

            // Functions and bindings
            Term::Lambda(x, param_ty, body) => self.infer_lambda_type(x, param_ty, body, local_ctx),
            Term::App(func, _) => self.infer_app_result_type(func, local_ctx),
            Term::Let(x, ty, _def, body) => self.infer_with_binding(x, ty, body, local_ctx),

            // Control flow
            Term::If(_, then_, else_) => self
                .infer_term_type_with_ctx(then_, local_ctx)
                .or_else(|_| self.infer_term_type_with_ctx(else_, local_ctx)),
            Term::Case(scrut, x_left, left, x_right, right) => {
                self.infer_case_type(scrut, (x_left, left), (x_right, right), local_ctx)
            }

            // Polymorphism
            Term::TyAbs(var, body) => self.infer_ty_abs_type(var, body, local_ctx),
            Term::TyApp(body, ty_arg) => self.infer_ty_app_type(body, ty_arg, local_ctx),

            // Recursive types
            Term::Unfold(mu_ty, _) => self.infer_unfold_type(mu_ty),

            // References
            Term::RefNew(val) => self.infer_ref_new_type(val, local_ctx),
            Term::RefGet(ref_term) => self.infer_ref_get_type(ref_term, local_ctx),

            // ADT
            Term::AdtMatch(scrutinee, arms) => {
                self.infer_adt_match_type(scrutinee, arms, local_ctx)
            }

            // Span wrapper: transparent to type inference
            Term::Spanned(inner, _) => self.infer_term_type_with_ctx(inner, local_ctx),

            // Early return: type is ⊥ (Void)
            Term::Return(_) => Ok(Type::Void),

            // Handled by infer_constant_type / infer_annotated_type above
            _ => unreachable!(),
        }
    }

    /// Resolve a variable's type from local context, environment, or top-level definitions.
    fn resolve_var_type(
        &self,
        x: &str,
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        if let Some(ty) = local_ctx.get(x) {
            return Ok(ty.clone());
        }
        if let Some((_, ty)) = self.compilation.env.get(x) {
            return Ok(ty.clone());
        }
        if let Some(ty) = self.defs.def_types.get(x) {
            return Ok(ty.clone());
        }
        Err(CodeGenError::UnboundVariable(x.to_string()))
    }

    /// Infer the result type of a function application.
    fn infer_app_result_type(
        &self,
        func: &Term,
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        let func_ty = self.infer_term_type_with_ctx(func, local_ctx)?;
        if let Type::Arrow(_, ret) = func_ty {
            Ok(ret.as_ref().clone())
        } else {
            Err(CodeGenError::TypeError(format!(
                "app on non-function: func={func:?}, inferred type={func_ty:?}"
            )))
        }
    }

    /// Infer the result type of a type application.
    fn infer_ty_app_type(
        &self,
        body: &Term,
        ty_arg: &Type,
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        let body_ty = self.infer_term_type_with_ctx(body, local_ctx)?;
        if let Type::Forall(var, inner) = body_ty {
            let resolved_ty_arg = self.types.apply_type_subst(ty_arg);
            Ok(inner.substitute(&var, &resolved_ty_arg))
        } else {
            Ok(body_ty)
        }
    }

    /// Infer the type of a product projection (fst or snd).
    fn infer_product_projection(
        &self,
        t: &Term,
        local_ctx: &HashMap<String, Type>,
        is_fst: bool,
    ) -> Result<Type, CodeGenError> {
        let raw_ty = self.infer_term_type_with_ctx(t, local_ctx)?;
        let label = if is_fst { "fst" } else { "snd" };
        // Try direct product match first
        if let Type::Product(ty1, ty2) = &raw_ty {
            return Ok(if is_fst { ty1 } else { ty2 }.as_ref().clone());
        }
        // Try expanding record/ADT types
        if let Some(expanded) = self.types.expand_type(&raw_ty) {
            if let Type::Product(ty1, ty2) = expanded {
                return Ok(if is_fst { ty1 } else { ty2 }.as_ref().clone());
            }
        }
        Err(CodeGenError::TypeError(format!(
            "{label} on non-product: {raw_ty:?}"
        )))
    }

    /// Infer the type of a Case expression.
    fn infer_case_type(
        &self,
        scrut: &Term,
        left: (&str, &Term),
        right: (&str, &Term),
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        if let Ok(scrut_ty) = self.infer_term_type_with_ctx(scrut, local_ctx) {
            if let Type::Sum(ty_l, ty_r) = scrut_ty {
                let mut left_ctx = local_ctx.clone();
                left_ctx.insert(left.0.to_owned(), ty_l.as_ref().clone());
                if let Ok(ty) = self.infer_term_type_with_ctx(left.1, &left_ctx) {
                    return Ok(ty);
                }
                let mut right_ctx = local_ctx.clone();
                right_ctx.insert(right.0.to_owned(), ty_r.as_ref().clone());
                return self.infer_term_type_with_ctx(right.1, &right_ctx);
            }
        }
        self.infer_term_type_with_ctx(left.1, local_ctx)
            .or_else(|_| self.infer_term_type_with_ctx(right.1, local_ctx))
    }

    /// Infer the type of a Global reference.
    fn infer_global_type(&self, name: &str) -> Result<Type, CodeGenError> {
        let lookup_name = self
            .defs
            .extern_name_map
            .get(name)
            .map_or(name, std::string::String::as_str);
        if let Some(ty) = self
            .defs
            .def_types
            .get(lookup_name)
            .or_else(|| self.defs.def_types.get(name))
        {
            return Ok(ty.clone());
        }
        Err(CodeGenError::UnboundVariable(name.to_owned()))
    }

    /// Infer the return type of an `ExternCall`.
    fn infer_extern_call_type(&self, symbol: &str) -> Result<Type, CodeGenError> {
        let wrapper_name = symbol.strip_prefix("__c_").unwrap_or(symbol);
        let llvm_name = self
            .defs
            .extern_name_map
            .get(wrapper_name)
            .map_or(wrapper_name, std::string::String::as_str);
        if let Some(ty) = self
            .defs
            .def_types
            .get(llvm_name)
            .or_else(|| self.defs.def_types.get(wrapper_name))
        {
            let mut current = ty.clone();
            while let Type::Arrow(_, ret) = current {
                current = ret.as_ref().clone();
            }
            return Ok(current);
        }
        Err(CodeGenError::Unsupported(format!(
            "cannot infer type of extern_call '{symbol}' - not in def_types"
        )))
    }

    /// Infer the type of an `AdtMatch` expression.
    fn infer_adt_match_type(
        &self,
        scrutinee: &Term,
        arms: &[(usize, String, Box<Term>)],
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        if let Some((_, var, body)) = arms.first() {
            let scrut_ty = self.infer_term_type_with_ctx(scrutinee, local_ctx)?;
            if let Type::Adt(_, _, variants) = scrut_ty {
                if let Some((_, payload_ty)) = variants.first() {
                    let mut arm_ctx = local_ctx.clone();
                    arm_ctx.insert(var.clone(), payload_ty.clone());
                    return self.infer_term_type_with_ctx(body, &arm_ctx);
                }
            }
            return self.infer_term_type_with_ctx(body, local_ctx);
        }
        Err(CodeGenError::TypeError("AdtMatch with no arms".to_string()))
    }
}

/// Returns the type for terms that always produce a known constant type.
fn infer_constant_type(term: &Term) -> Option<Type> {
    match term {
        // Bool
        Term::True
        | Term::False
        | Term::StrEq(_, _)
        | Term::NatLt(_, _)
        | Term::NatLe(_, _)
        | Term::NatGt(_, _)
        | Term::NatGe(_, _)
        | Term::NatEq(_, _)
        | Term::BoolAnd(_, _)
        | Term::BoolOr(_, _)
        | Term::BoolNot(_) => Some(Type::Bool),

        // Nat
        Term::Zero
        | Term::Succ(_)
        | Term::NatLit(_)
        | Term::StrLen(_)
        | Term::StrCharAt(_, _)
        | Term::NatAdd(_, _)
        | Term::NatSub(_, _)
        | Term::NatMul(_, _)
        | Term::NatDiv(_, _)
        | Term::NatMod(_, _) => Some(Type::Nat),

        // String
        Term::StringLit(_) | Term::StrConcat(_, _) | Term::StrSubstring(_, _, _) => {
            Some(Type::String)
        }

        // Unit
        Term::Unit
        | Term::Refl(_, _)
        | Term::Subst(_, _, _, _)
        | Term::Sorry
        | Term::RefSet(_, _) => Some(Type::Unit),

        _ => None,
    }
}

/// Returns the type for terms that carry their own type annotation.
fn infer_annotated_type(term: &Term) -> Option<&Type> {
    match term {
        Term::Inl(ty, _) | Term::Inr(ty, _) => Some(ty),
        Term::NatRec(ty, _, _, _) | Term::NatInd(ty, _, _, _) => Some(ty),
        Term::Fix(_, ty, _) => Some(ty),
        Term::Fold(ty, _) => Some(ty),
        Term::AdtConstruct(ty, _, _) => Some(ty),
        Term::Annot(_, ty) => Some(ty),
        Term::Absurd(ty, _) => Some(ty),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Simple type-inference helpers
// ─────────────────────────────────────────────────────────────────────────

impl CodeGen<'_> {
    /// Infer the type of a pair constructor.
    pub(in crate::codegen::exec) fn infer_pair_type(
        &self,
        t1: &Term,
        t2: &Term,
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        let ty1 = self.infer_term_type_with_ctx(t1, local_ctx)?;
        let ty2 = self.infer_term_type_with_ctx(t2, local_ctx)?;
        Ok(Type::product(ty1, ty2))
    }

    /// Infer the type of a lambda abstraction.
    pub(in crate::codegen::exec) fn infer_lambda_type(
        &self,
        x: &str,
        param_ty: &Type,
        body: &Term,
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        let mut extended_ctx = local_ctx.clone();
        extended_ctx.insert(x.to_owned(), param_ty.clone());
        let body_ty = self.infer_term_type_with_ctx(body, &extended_ctx)?;
        Ok(Type::arrow(param_ty.clone(), body_ty))
    }

    /// Infer the type of a let-binding by extending the context.
    pub(in crate::codegen::exec) fn infer_with_binding(
        &self,
        x: &str,
        ty: &Type,
        body: &Term,
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        let mut extended_ctx = local_ctx.clone();
        extended_ctx.insert(x.to_owned(), ty.clone());
        self.infer_term_type_with_ctx(body, &extended_ctx)
    }

    /// Infer the result of unfolding a recursive (mu) type.
    pub(in crate::codegen::exec) fn infer_unfold_type(
        &self,
        mu_ty: &Type,
    ) -> Result<Type, CodeGenError> {
        // Unwrap all nested Mu binders. For mutually recursive types, the
        // type annotation carries multiple Mu layers (one per SCC member),
        // and unfold peels through all of them to reach the inner type.
        let mut current = mu_ty.clone();
        while let Type::Mu(ref var, ref body) = current {
            current = body.substitute(var, &current);
        }
        if &current == mu_ty {
            Err(CodeGenError::TypeError("unfold on non-mu type".to_string()))
        } else {
            Ok(current)
        }
    }

    /// Infer the type of dereferencing a reference.
    pub(in crate::codegen::exec) fn infer_ref_get_type(
        &self,
        ref_term: &Term,
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        match self.infer_term_type_with_ctx(ref_term, local_ctx)? {
            Type::Ref(inner) => Ok(*inner),
            _ => Err(CodeGenError::TypeError("ref_get on non-ref".to_string())),
        }
    }

    /// Infer the type of a type abstraction.
    pub(in crate::codegen::exec) fn infer_ty_abs_type(
        &self,
        var: &str,
        body: &Term,
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        let body_ty = self.infer_term_type_with_ctx(body, local_ctx)?;
        Ok(Type::Forall(var.to_owned(), Box::new(body_ty)))
    }

    /// Infer the type of creating a new reference.
    pub(in crate::codegen::exec) fn infer_ref_new_type(
        &self,
        val: &Term,
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        let inner_ty = self.infer_term_type_with_ctx(val, local_ctx)?;
        Ok(Type::Ref(Box::new(inner_ty)))
    }
}

#[cfg(test)]
mod tests;
