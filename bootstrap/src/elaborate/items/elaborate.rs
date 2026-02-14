//! Pass 2: Definition Elaboration
//!
//! Elaborates top-level items to Core definitions by building
//! Core terms from item bodies.

use crate::ast;
use crate::span::Spanned;

use tungsten_core::Term;

use crate::elaborate::error::{ElabError, ElabErrorKind, ExpectedContext};
use crate::elaborate::{CoreDef, ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    pub(super) fn elaborate_function(&mut self, func: &ast::FunctionDef) -> ElabResult<CoreDef> {
        // Get the type we computed in pass 1
        let name = func.name.name.clone();
        let span = func.name.span;
        let func_ty = self
            .env
            .lookup_value(&name)
            .ok_or_else(|| ElabError::new(span, ElabErrorKind::UndefinedVariable(name.clone())))?
            .ty
            .clone();

        // Push type parameters
        for tp in &func.type_params {
            self.env.push_type_var(tp.name.name.clone());
        }

        // Build the term: Λα. ... λ(x:T). body
        let mut term = self.elaborate_function_body(func)?;

        // Wrap in type abstractions
        for tp in func.type_params.iter().rev() {
            term = Term::TyAbs(tp.name.name.clone(), Box::new(term));
        }

        // Pop type parameters
        for _ in &func.type_params {
            self.env.pop_type_var();
        }

        Ok(CoreDef {
            name: func.name.name.clone(),
            ty: func_ty,
            term,
            span: func.span,
        })
    }

    fn elaborate_function_body(&mut self, func: &ast::FunctionDef) -> ElabResult<Term> {
        self.env.push_scope();

        // Bind all parameters
        let mut param_types = Vec::new();
        for param in &func.params {
            let name = self.pattern_to_name(&param.pattern)?;
            let ty = self.elab_type(&param.ty)?;
            self.env.bind_local(name.clone(), ty.clone(), self.depth);
            self.depth += 1;
            param_types.push((name, ty));
        }

        // Elaborate body
        let body_term = if let Some(ref ret_ty) = func.return_type {
            let expected = self.elab_type(ret_ty)?;
            // Push context for better error messages
            self.push_context(ExpectedContext::return_type(ret_ty.span()));
            let result = self.check(&func.body, &expected);
            self.pop_context();
            result?
        } else {
            self.infer(&func.body)?.0
        };

        // Build nested lambdas
        let mut term = body_term;
        for (name, ty) in param_types.into_iter().rev() {
            self.depth -= 1;
            term = Term::lambda(name, ty, term);
        }

        self.env.pop_scope();
        Ok(term)
    }

    pub(super) fn elaborate_theorem(&mut self, thm: &ast::TheoremDef) -> ElabResult<CoreDef> {
        // Get type from pass 1
        let name = thm.name.name.clone();
        let span = thm.name.span;
        let thm_ty = self
            .env
            .lookup_value(&name)
            .ok_or_else(|| ElabError::new(span, ElabErrorKind::UndefinedVariable(name.clone())))?
            .ty
            .clone();

        // Push type parameters
        for tp in &thm.type_params {
            self.env.push_type_var(tp.name.name.clone());
        }

        // Elaborate theorem body (proof)
        let term = self.elaborate_theorem_body(thm)?;

        // Wrap in type abstractions
        let mut wrapped_term = term;
        for tp in thm.type_params.iter().rev() {
            wrapped_term = Term::TyAbs(tp.name.name.clone(), Box::new(wrapped_term));
        }

        // Pop type parameters
        for _ in &thm.type_params {
            self.env.pop_type_var();
        }

        Ok(CoreDef {
            name: thm.name.name.clone(),
            ty: thm_ty,
            term: wrapped_term,
            span: thm.span,
        })
    }

    fn elaborate_theorem_body(&mut self, thm: &ast::TheoremDef) -> ElabResult<Term> {
        self.env.push_scope();

        // Bind all parameters (hypotheses)
        let mut param_types = Vec::new();
        for param in &thm.params {
            let name = self.pattern_to_name(&param.pattern)?;
            let ty = self.elab_type(&param.ty)?;
            self.env.bind_local(name.clone(), ty.clone(), self.depth);
            self.depth += 1;
            param_types.push((name, ty));
        }

        // Elaborate proof body against the proposition
        let expected = self.elab_type(&thm.prop)?;
        let body_term = self.check(&thm.body, &expected)?;

        // Build nested lambdas for parameters
        let mut term = body_term;
        for (name, ty) in param_types.into_iter().rev() {
            self.depth -= 1;
            term = Term::lambda(name, ty, term);
        }

        self.env.pop_scope();
        Ok(term)
    }

    pub(super) fn elaborate_axiom(&mut self, axiom: &ast::AxiomDef) -> ElabResult<CoreDef> {
        // Get type from pass 1
        let name = axiom.name.name.clone();
        let span = axiom.name.span;
        let axiom_ty = self
            .env
            .lookup_value(&name)
            .ok_or_else(|| ElabError::new(span, ElabErrorKind::UndefinedVariable(name.clone())))?
            .ty
            .clone();

        // Axioms have no proof - they use sorry
        // Build: Λα. ... λ(h:P). sorry
        let mut term = Term::Sorry;

        // Push type parameters
        for tp in &axiom.type_params {
            self.env.push_type_var(tp.name.name.clone());
        }

        // Wrap in lambdas for parameters
        for param in axiom.params.iter().rev() {
            let _name = self.pattern_to_name(&param.pattern)?;
            let ty = self.elab_type(&param.ty)?;
            // For axioms, we don't actually need to bind - just wrap in lambda
            term = Term::lambda("_", ty, term);
        }

        // Wrap in type abstractions
        for tp in axiom.type_params.iter().rev() {
            term = Term::TyAbs(tp.name.name.clone(), Box::new(term));
        }

        // Pop type parameters
        for _ in &axiom.type_params {
            self.env.pop_type_var();
        }

        Ok(CoreDef {
            name: axiom.name.name.clone(),
            ty: axiom_ty,
            term,
            span: axiom.span,
        })
    }
}
