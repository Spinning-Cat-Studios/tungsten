//! Type Building Helpers
//!
//! Constructs Core types from AST item signatures.

use crate::ast;

use tungsten_core::Type;

use crate::elaborate::ElabResult;
use crate::elaborate::Elaborator;

impl<'a> Elaborator<'a> {
    /// Build the type of a function from its signature.
    pub(super) fn build_function_type(&mut self, func: &ast::FunctionDef) -> ElabResult<Type> {
        // Push type parameters for elaboration
        for tp in &func.type_params {
            self.env.push_type_var(tp.name.name.clone());
        }

        // Build parameter types → return type
        let return_ty = if let Some(ref ret) = func.return_type {
            self.elab_type(ret)?
        } else {
            // No return type annotation: will be inferred during elaboration
            // For now, use Unit as placeholder (this will be refined)
            Type::Unit
        };

        // Build curried function type: A → B → C → Result
        let mut ty = return_ty;
        for param in func.params.iter().rev() {
            let param_ty = self.elab_type(&param.ty)?;
            ty = Type::arrow(param_ty, ty);
        }

        // Wrap in forall for type parameters
        for tp in func.type_params.iter().rev() {
            ty = Type::forall(&tp.name.name, ty);
        }

        // Pop type parameters
        for _ in &func.type_params {
            self.env.pop_type_var();
        }

        Ok(ty)
    }

    /// Build the type of a theorem from its signature.
    pub(super) fn build_theorem_type(&mut self, thm: &ast::TheoremDef) -> ElabResult<Type> {
        // Push type parameters
        for tp in &thm.type_params {
            self.env.push_type_var(tp.name.name.clone());
        }

        // Build: (param_types) → prop
        let prop_ty = self.elab_type(&thm.prop)?;

        let mut ty = prop_ty;
        for param in thm.params.iter().rev() {
            let param_ty = self.elab_type(&param.ty)?;
            ty = Type::arrow(param_ty, ty);
        }

        // Wrap in forall for type parameters
        for tp in thm.type_params.iter().rev() {
            ty = Type::forall(&tp.name.name, ty);
        }

        // Pop type parameters
        for _ in &thm.type_params {
            self.env.pop_type_var();
        }

        Ok(ty)
    }

    /// Build the type of an axiom from its signature.
    pub(super) fn build_axiom_type(&mut self, axiom: &ast::AxiomDef) -> ElabResult<Type> {
        // Push type parameters
        for tp in &axiom.type_params {
            self.env.push_type_var(tp.name.name.clone());
        }

        // Build: (param_types) → prop
        let prop_ty = self.elab_type(&axiom.prop)?;

        let mut ty = prop_ty;
        for param in axiom.params.iter().rev() {
            let param_ty = self.elab_type(&param.ty)?;
            ty = Type::arrow(param_ty, ty);
        }

        // Wrap in forall for type parameters
        for tp in axiom.type_params.iter().rev() {
            ty = Type::forall(&tp.name.name, ty);
        }

        // Pop type parameters
        for _ in &axiom.type_params {
            self.env.pop_type_var();
        }

        Ok(ty)
    }
}
