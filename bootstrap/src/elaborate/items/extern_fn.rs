//! FFI: External Functions (Phase 3-Prep)
//!
//! Handles collection and elaboration of extern function declarations.

use crate::ast;

use tungsten_core::terms::{SpannedTerm, TermSpan};
use tungsten_core::{Term, Type};

use crate::elaborate::env::ValueDef;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{CoreDef, ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Collect an extern function declaration (first pass).
    pub(super) fn collect_extern_fn(&mut self, extern_fn: &ast::ExternFnDef) -> ElabResult<()> {
        // Check for duplicate (allow overwrite if Phase A stub, ADR 5.5.26c)
        if self.env.has_value(&extern_fn.name.name) && !self.allow_value_overwrite {
            return Err(ElabError::duplicate(
                extern_fn.name.span,
                &extern_fn.name.name,
            ));
        }

        // Build function type from params and return type
        let ty = self.build_extern_fn_type(extern_fn)?;

        self.env.define_value(ValueDef {
            name: extern_fn.name.name.clone(),
            ty,
            visibility: extern_fn.visibility,
            span: extern_fn.span,
        });

        Ok(())
    }

    /// Build the type of an extern function: (param_types) → return_type
    fn build_extern_fn_type(&mut self, extern_fn: &ast::ExternFnDef) -> ElabResult<Type> {
        // Elaborate return type
        let return_ty = self.elab_type(&extern_fn.return_type)?;

        // Build curried function type: P1 → P2 → ... → Ret
        let mut ty = return_ty;
        for param in extern_fn.params.iter().rev() {
            let param_ty = self.elab_type(&param.ty)?;
            ty = Type::arrow(param_ty, ty);
        }

        Ok(ty)
    }

    /// Elaborate an extern function to a CoreDef.
    ///
    /// We create a wrapper function that calls the extern symbol.
    pub(super) fn elaborate_extern_fn(
        &mut self,
        extern_fn: &ast::ExternFnDef,
    ) -> ElabResult<CoreDef> {
        let name = extern_fn.name.name.clone();
        // Use the explicit symbol name if provided, otherwise use the function name
        // Prefix with "__c_" to avoid collision with the wrapper function
        let raw_symbol = extern_fn.symbol.clone().unwrap_or_else(|| name.clone());
        let symbol = format!("__c_{}", raw_symbol);

        // Get the type we computed in pass 1
        let func_ty = self
            .env
            .lookup_value(&name)
            .ok_or_else(|| {
                ElabError::new(
                    extern_fn.name.span,
                    ElabErrorKind::UndefinedVariable(name.clone()),
                )
            })?
            .ty
            .clone();

        // Build a wrapper term that packages arguments and calls extern
        // For extern "C" fn foo(x: Nat, y: String) -> Bool
        // We generate: λx:Nat. λy:String. extern_call "foo" [x, y]
        let mut param_names = Vec::new();
        let mut param_types = Vec::new();

        for param in &extern_fn.params {
            let param_ty = self.elab_type(&param.ty)?;
            param_names.push(param.name.name.clone());
            param_types.push(param_ty);
        }

        // Build extern_call term with variable references
        let args: Vec<Term> = param_names.iter().map(|n| Term::var(n)).collect();
        let mut term = Term::extern_call(symbol, args);

        // Wrap in lambdas
        for (pname, pty) in param_names.iter().rev().zip(param_types.iter().rev()) {
            term = Term::lambda(pname, pty.clone(), term);
        }

        Ok(CoreDef {
            name,
            ty: func_ty,
            term: SpannedTerm::new(
                term,
                TermSpan::new(extern_fn.span.start, extern_fn.span.end),
            ),
            span: extern_fn.span,
        })
    }
}
