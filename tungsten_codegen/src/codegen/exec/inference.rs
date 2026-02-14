//! Type inference for code generation.
//!
//! Provides simplified type inference for terms during compilation.
//! Requires type annotations for complex cases.

use crate::codegen::error::CodeGenError;
use crate::codegen::CodeGen;
use std::collections::HashMap;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
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
        match term {
            // ═══════════════════════════════════════════════════════════════════
            // Variables
            // ═══════════════════════════════════════════════════════════════════
            Term::Var(x) => {
                // First check local context (Let/Lambda/Case bindings)
                if let Some(ty) = local_ctx.get(x) {
                    return Ok(ty.clone());
                }
                // Then check current environment
                if let Some((_, ty)) = self.env.get(x) {
                    return Ok(ty.clone());
                }
                // Then check top-level definitions
                if let Some(ty) = self.def_types.get(x) {
                    return Ok(ty.clone());
                }
                Err(CodeGenError::UnboundVariable(x.clone()))
            }

            // ═══════════════════════════════════════════════════════════════════
            // Primitives
            // ═══════════════════════════════════════════════════════════════════
            Term::True | Term::False => Ok(Type::Bool),
            Term::Zero | Term::Succ(_) | Term::NatLit(_) => Ok(Type::Nat),
            Term::Unit => Ok(Type::Unit),

            // ═══════════════════════════════════════════════════════════════════
            // Strings
            // ═══════════════════════════════════════════════════════════════════
            Term::StringLit(_) | Term::StrConcat(_, _) | Term::StrSubstring(_, _, _) => {
                Ok(Type::String)
            }
            Term::StrLen(_) | Term::StrCharAt(_, _) => Ok(Type::Nat),
            Term::StrEq(_, _) => Ok(Type::Bool),

            // ═══════════════════════════════════════════════════════════════════
            // Products
            // ═══════════════════════════════════════════════════════════════════
            Term::Pair(t1, t2) => {
                let ty1 = self.infer_term_type_with_ctx(t1, local_ctx)?;
                let ty2 = self.infer_term_type_with_ctx(t2, local_ctx)?;
                Ok(Type::product(ty1, ty2))
            }
            Term::Fst(t) => {
                let raw_ty = self.infer_term_type_with_ctx(t, local_ctx)?;
                // Try direct product match first
                if let Type::Product(ty1, _) = &raw_ty {
                    return Ok(ty1.as_ref().clone());
                }
                // Try expanding record/ADT types
                if let Some(expanded) = self.types.expand_type(&raw_ty) {
                    if let Type::Product(ty1, _) = expanded {
                        return Ok(ty1.as_ref().clone());
                    }
                }
                Err(CodeGenError::TypeError(format!(
                    "fst on non-product: {:?}",
                    raw_ty
                )))
            }
            Term::Snd(t) => {
                let raw_ty = self.infer_term_type_with_ctx(t, local_ctx)?;
                // Try direct product match first
                if let Type::Product(_, ty2) = &raw_ty {
                    return Ok(ty2.as_ref().clone());
                }
                // Try expanding record/ADT types
                if let Some(expanded) = self.types.expand_type(&raw_ty) {
                    if let Type::Product(_, ty2) = expanded {
                        return Ok(ty2.as_ref().clone());
                    }
                }
                Err(CodeGenError::TypeError(format!(
                    "snd on non-product: {:?}",
                    raw_ty
                )))
            }

            // ═══════════════════════════════════════════════════════════════════
            // Sums
            // ═══════════════════════════════════════════════════════════════════
            Term::Inl(sum_ty, _) | Term::Inr(sum_ty, _) => Ok(sum_ty.clone()),

            // ═══════════════════════════════════════════════════════════════════
            // Functions
            // ═══════════════════════════════════════════════════════════════════
            Term::Lambda(x, param_ty, body) => {
                // Extend context with the lambda parameter
                let mut extended_ctx = local_ctx.clone();
                extended_ctx.insert(x.clone(), param_ty.clone());
                let body_ty = self.infer_term_type_with_ctx(body, &extended_ctx)?;
                Ok(Type::arrow(param_ty.clone(), body_ty))
            }
            Term::App(func, _) => {
                let func_ty = self.infer_term_type_with_ctx(func, local_ctx)?;
                if let Type::Arrow(_, ret) = func_ty {
                    Ok(ret.as_ref().clone())
                } else {
                    Err(CodeGenError::TypeError(format!(
                        "app on non-function: func={:?}, inferred type={:?}",
                        func, func_ty
                    )))
                }
            }
            Term::Let(x, ty, _def, body) => {
                // Extend context with the Let binding
                let mut extended_ctx = local_ctx.clone();
                extended_ctx.insert(x.clone(), ty.clone());
                self.infer_term_type_with_ctx(body, &extended_ctx)
            }

            // ═══════════════════════════════════════════════════════════════════
            // Control Flow
            // ═══════════════════════════════════════════════════════════════════
            Term::If(_, then_, else_) => {
                // Try then branch first, fall back to else
                self.infer_term_type_with_ctx(then_, local_ctx)
                    .or_else(|_| self.infer_term_type_with_ctx(else_, local_ctx))
            }
            Term::Case(scrut, x_left, left, x_right, right) => {
                // Infer scrutinee type to get sum type components
                if let Ok(scrut_ty) = self.infer_term_type_with_ctx(scrut, local_ctx) {
                    if let Type::Sum(ty_l, ty_r) = scrut_ty {
                        // Try left with binding
                        let mut left_ctx = local_ctx.clone();
                        left_ctx.insert(x_left.clone(), ty_l.as_ref().clone());
                        if let Ok(ty) = self.infer_term_type_with_ctx(left, &left_ctx) {
                            return Ok(ty);
                        }
                        // Try right with binding
                        let mut right_ctx = local_ctx.clone();
                        right_ctx.insert(x_right.clone(), ty_r.as_ref().clone());
                        return self.infer_term_type_with_ctx(right, &right_ctx);
                    }
                }
                // Fallback: try without bindings
                self.infer_term_type_with_ctx(left, local_ctx)
                    .or_else(|_| self.infer_term_type_with_ctx(right, local_ctx))
            }
            Term::NatRec(result_ty, _, _, _) | Term::NatInd(result_ty, _, _, _) => {
                Ok(result_ty.clone())
            }

            // ═══════════════════════════════════════════════════════════════════
            // Equality (proof erasure)
            // ═══════════════════════════════════════════════════════════════════
            Term::Refl(_, _) | Term::Subst(_, _, _, _) => Ok(Type::Unit),

            // ═══════════════════════════════════════════════════════════════════
            // Polymorphism
            // ═══════════════════════════════════════════════════════════════════
            Term::TyAbs(var, body) => {
                let body_ty = self.infer_term_type_with_ctx(body, local_ctx)?;
                Ok(Type::Forall(var.clone(), Box::new(body_ty)))
            }
            Term::TyApp(body, ty_arg) => {
                let body_ty = self.infer_term_type_with_ctx(body, local_ctx)?;
                if let Type::Forall(var, inner) = body_ty {
                    // Apply current type substitution to ty_arg (for monomorphization context)
                    let resolved_ty_arg = self.types.apply_type_subst(ty_arg);
                    // Substitute resolved ty_arg for var in inner
                    Ok(inner.substitute(&var, &resolved_ty_arg))
                } else {
                    // Already instantiated or not polymorphic
                    Ok(body_ty)
                }
            }

            // ═══════════════════════════════════════════════════════════════════
            // Recursion and recursive types
            // ═══════════════════════════════════════════════════════════════════
            Term::Fix(_, ty, _) => Ok(ty.clone()),
            Term::Fold(mu_ty, _) => Ok(mu_ty.clone()),
            Term::Unfold(mu_ty, _) => {
                if let Type::Mu(var, body) = mu_ty {
                    Ok(body.substitute(var, mu_ty))
                } else {
                    Err(CodeGenError::TypeError("unfold on non-mu type".to_string()))
                }
            }

            // ═══════════════════════════════════════════════════════════════════
            // Meta
            // ═══════════════════════════════════════════════════════════════════
            Term::Annot(_, ty) => Ok(ty.clone()),
            Term::Absurd(ty, _) => Ok(ty.clone()),
            Term::Sorry => Ok(Type::Unit),

            // ═══════════════════════════════════════════════════════════════════
            // Globals and externs
            // ═══════════════════════════════════════════════════════════════════
            Term::Global(name) => {
                // Check if name is remapped (extern wrappers are renamed)
                let lookup_name = self
                    .extern_name_map
                    .get(name)
                    .map(|s| s.as_str())
                    .unwrap_or(name.as_str());
                if let Some(ty) = self
                    .def_types
                    .get(lookup_name)
                    .or_else(|| self.def_types.get(name))
                {
                    return Ok(ty.clone());
                }
                Err(CodeGenError::UnboundVariable(name.clone()))
            }
            Term::ExternCall(symbol, _) => {
                // Strip __c_ prefix if present to look up the wrapper's type
                let wrapper_name = if symbol.starts_with("__c_") {
                    &symbol[4..]
                } else {
                    symbol.as_str()
                };

                // Look up extern type from def_types
                let llvm_name = self
                    .extern_name_map
                    .get(wrapper_name)
                    .map(|s| s.as_str())
                    .unwrap_or(wrapper_name);

                if let Some(ty) = self
                    .def_types
                    .get(llvm_name)
                    .or_else(|| self.def_types.get(wrapper_name))
                {
                    // Extract return type from function type (peel off all arrows)
                    let mut current = ty.clone();
                    while let Type::Arrow(_, ret) = current {
                        current = ret.as_ref().clone();
                    }
                    return Ok(current);
                }
                Err(CodeGenError::Unsupported(format!(
                    "cannot infer type of extern_call '{}' - not in def_types",
                    symbol
                )))
            }

            // ═══════════════════════════════════════════════════════════════════
            // Natural number operations
            // ═══════════════════════════════════════════════════════════════════
            Term::NatLt(_, _) | Term::NatLe(_, _) | Term::NatGt(_, _) | Term::NatGe(_, _) => {
                Ok(Type::Bool)
            }
            Term::NatAdd(_, _)
            | Term::NatSub(_, _)
            | Term::NatMul(_, _)
            | Term::NatDiv(_, _)
            | Term::NatMod(_, _) => Ok(Type::Nat),
            Term::NatEq(_, _) => Ok(Type::Bool),

            // ═══════════════════════════════════════════════════════════════════
            // Boolean operations
            // ═══════════════════════════════════════════════════════════════════
            Term::BoolAnd(_, _) | Term::BoolOr(_, _) | Term::BoolNot(_) => Ok(Type::Bool),

            // ═══════════════════════════════════════════════════════════════════
            // References
            // ═══════════════════════════════════════════════════════════════════
            Term::RefNew(val) => {
                let inner_ty = self.infer_term_type_with_ctx(val, local_ctx)?;
                Ok(Type::Ref(Box::new(inner_ty)))
            }
            Term::RefGet(ref_term) => match self.infer_term_type_with_ctx(ref_term, local_ctx)? {
                Type::Ref(inner) => Ok(*inner),
                _ => Err(CodeGenError::TypeError("ref_get on non-ref".to_string())),
            },
            Term::RefSet(_, _) => Ok(Type::Unit),

            // ═══════════════════════════════════════════════════════════════════
            // ADT (flat enum)
            // ═══════════════════════════════════════════════════════════════════
            Term::AdtConstruct(adt_ty, _, _) => Ok(adt_ty.clone()),
            Term::AdtMatch(scrutinee, arms) => {
                // The result type is the type of any arm body
                if let Some((_, var, body)) = arms.first() {
                    let scrut_ty = self.infer_term_type_with_ctx(scrutinee, local_ctx)?;
                    if let Type::Adt(_, _, variants) = scrut_ty {
                        if let Some((_, payload_ty)) = variants.first() {
                            let mut arm_ctx = local_ctx.clone();
                            arm_ctx.insert(var.clone(), payload_ty.clone());
                            return self.infer_term_type_with_ctx(body, &arm_ctx);
                        }
                    }
                    // Fallback: try without the binding
                    return self.infer_term_type_with_ctx(body, local_ctx);
                }
                Err(CodeGenError::TypeError("AdtMatch with no arms".to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    fn setup_codegen(context: &Context) -> CodeGen {
        CodeGen::new(context, "test")
    }

    #[test]
    fn test_infer_bool() {
        let context = Context::create();
        let codegen = setup_codegen(&context);

        assert_eq!(codegen.infer_term_type(&Term::True).unwrap(), Type::Bool);
        assert_eq!(codegen.infer_term_type(&Term::False).unwrap(), Type::Bool);
    }

    #[test]
    fn test_infer_nat() {
        let context = Context::create();
        let codegen = setup_codegen(&context);

        assert_eq!(codegen.infer_term_type(&Term::Zero).unwrap(), Type::Nat);
        assert_eq!(
            codegen
                .infer_term_type(&Term::Succ(Box::new(Term::Zero)))
                .unwrap(),
            Type::Nat
        );
        assert_eq!(
            codegen.infer_term_type(&Term::NatLit(42)).unwrap(),
            Type::Nat
        );
    }

    #[test]
    fn test_infer_unit() {
        let context = Context::create();
        let codegen = setup_codegen(&context);

        assert_eq!(codegen.infer_term_type(&Term::Unit).unwrap(), Type::Unit);
    }

    #[test]
    fn test_infer_string() {
        let context = Context::create();
        let codegen = setup_codegen(&context);

        assert_eq!(
            codegen
                .infer_term_type(&Term::StringLit("hello".to_string()))
                .unwrap(),
            Type::String
        );
    }

    #[test]
    fn test_infer_pair() {
        let context = Context::create();
        let codegen = setup_codegen(&context);

        let pair = Term::Pair(Box::new(Term::True), Box::new(Term::Zero));
        let ty = codegen.infer_term_type(&pair).unwrap();
        assert_eq!(ty, Type::product(Type::Bool, Type::Nat));
    }

    #[test]
    fn test_infer_lambda() {
        let context = Context::create();
        let codegen = setup_codegen(&context);

        let lambda = Term::Lambda(
            "x".to_string(),
            Type::Nat,
            Box::new(Term::Var("x".to_string())),
        );
        let ty = codegen.infer_term_type(&lambda).unwrap();
        assert_eq!(ty, Type::arrow(Type::Nat, Type::Nat));
    }

    #[test]
    fn test_infer_let() {
        let context = Context::create();
        let codegen = setup_codegen(&context);

        let let_term = Term::Let(
            "x".to_string(),
            Type::Nat,
            Box::new(Term::NatLit(42)),
            Box::new(Term::Var("x".to_string())),
        );
        let ty = codegen.infer_term_type(&let_term).unwrap();
        assert_eq!(ty, Type::Nat);
    }

    #[test]
    fn test_infer_nat_ops() {
        let context = Context::create();
        let codegen = setup_codegen(&context);

        let add = Term::NatAdd(Box::new(Term::NatLit(1)), Box::new(Term::NatLit(2)));
        assert_eq!(codegen.infer_term_type(&add).unwrap(), Type::Nat);

        let eq = Term::NatEq(Box::new(Term::NatLit(1)), Box::new(Term::NatLit(2)));
        assert_eq!(codegen.infer_term_type(&eq).unwrap(), Type::Bool);

        let lt = Term::NatLt(Box::new(Term::NatLit(1)), Box::new(Term::NatLit(2)));
        assert_eq!(codegen.infer_term_type(&lt).unwrap(), Type::Bool);
    }

    #[test]
    fn test_infer_bool_ops() {
        let context = Context::create();
        let codegen = setup_codegen(&context);

        let and = Term::BoolAnd(Box::new(Term::True), Box::new(Term::False));
        assert_eq!(codegen.infer_term_type(&and).unwrap(), Type::Bool);

        let not = Term::BoolNot(Box::new(Term::True));
        assert_eq!(codegen.infer_term_type(&not).unwrap(), Type::Bool);
    }

    #[test]
    fn test_infer_annot() {
        let context = Context::create();
        let codegen = setup_codegen(&context);

        let annot = Term::Annot(Box::new(Term::NatLit(42)), Type::Nat);
        assert_eq!(codegen.infer_term_type(&annot).unwrap(), Type::Nat);
    }

    #[test]
    fn test_infer_unbound_var() {
        let context = Context::create();
        let codegen = setup_codegen(&context);

        let result = codegen.infer_term_type(&Term::Var("x".to_string()));
        assert!(result.is_err());
    }
}
