//! Nullary constructor elaboration.
//!
//! Handles constructors with no arguments like `Nil` or `None`.

use crate::elaborate::env;
use crate::elaborate::{ElabResult, Elaborator};
use crate::span::Span;
use tungsten_core::{Term, Type};

impl<'a> Elaborator<'a> {
    /// Elaborate a constructor reference (nullary constructor like `Nil`).
    pub(in crate::elaborate) fn elab_constructor_ref(
        &mut self,
        name: &str,
        info: &env::ConstructorInfo,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // Validate nullary
        self.validate_ctor_arity(name, info.arity, 0, span)?;

        // Get type context
        let ctx = self.get_constructor_context(info, span)?;

        // Get the full ADT type (encoded as sum or μ-type)
        let adt_type = self.encode_adt_type(&info.type_name, &[])?;

        // Build the constructor term
        let term = self.build_constructor_term(
            Term::Unit,
            info.index,
            ctx.constructors.len(),
            &adt_type,
            ctx.is_recursive,
        )?;

        Ok((term, adt_type))
    }

    /// Check a constructor reference against an expected type.
    /// This is used when we know the expected type (e.g., function return type).
    pub(in crate::elaborate) fn check_constructor_ref(
        &mut self,
        name: &str,
        info: &env::ConstructorInfo,
        expected: &Type,
        span: Span,
    ) -> ElabResult<Term> {
        // Validate nullary
        self.validate_ctor_arity(name, info.arity, 0, span)?;

        // Get type context
        let ctx = self.get_constructor_context(info, span)?;

        // Use expected type directly as the ADT type
        let term = self.build_constructor_term(
            Term::Unit,
            info.index,
            ctx.constructors.len(),
            expected,
            ctx.is_recursive,
        )?;

        Ok(term)
    }
}
