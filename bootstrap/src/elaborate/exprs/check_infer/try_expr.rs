//! Elaboration of the `?` (try) operator (ADR 13.5.26e).
//!
//! `expr?` desugars to:
//!   - `Result<T, E>`: `match expr { Ok(v) => v, Err(e) => return Err(e) }`
//!   - `Option<T>`:    `match expr { Some(v) => v, None() => return None() }`
//!
//! The desugaring is done at the Core term level, not by re-parsing.

use crate::span::Span;
use tungsten_core::{Term, Type};

use crate::elaborate::env::TypeDefKind;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

/// Unfold a μ-type by substituting the Mu variable with the whole type.
/// Returns a clone for non-Mu types.
fn unfold_mu(ty: &Type) -> Type {
    match ty {
        Type::Mu(var, body) => body.substitute(var, ty),
        other => other.clone(),
    }
}

/// What kind of try-able type we detected.
enum TryKind {
    /// Result<T, E> — Ok is inr, Err is inl
    Result,
    /// Option<T> — Some is inr, None is inl
    Option,
}

impl<'a> Elaborator<'a> {
    /// Elaborate `expr?` — desugar into match + early return.
    pub(super) fn elab_try(
        &mut self,
        inner: &crate::ast::Expr,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // 1. Check we're inside a function with a known return type.
        //    Note: TryOutsideReturnContext is a defensive guard — the parser
        //    currently rejects `?` at module scope before elaboration runs.
        //    Kept for future contexts (REPL, eval) where `?` may parse without
        //    an enclosing function.
        let ret_ty = match &self.current_return_type {
            Some(ty) => ty.clone(),
            None => {
                return Err(ElabError::new(span, ElabErrorKind::TryOutsideReturnContext));
            }
        };

        // 2. Infer the operand's type
        let (operand_term, operand_type) = self.infer(inner)?;

        // 3. Detect whether it's Result or Option by looking up constructors
        let (try_kind, success_type) = self.classify_try_type(&operand_type, span)?;

        // 4. Check return type compatibility
        self.check_try_return_type(&try_kind, &operand_type, &ret_ty, span)?;

        // 5. Build the desugared match + return Core term
        let term =
            self.build_try_desugaring(&try_kind, operand_term, &operand_type, &ret_ty, span)?;

        Ok((term, success_type))
    }

    /// Classify the operand type as Result or Option.
    ///
    /// Checks both constructor presence in env AND operand type structure.
    /// This prevents `?` on `Nat` from being misclassified as Result when
    /// `Ok`/`Err` constructors happen to be in scope from a type definition.
    fn classify_try_type(&self, operand_type: &Type, span: Span) -> ElabResult<(TryKind, Type)> {
        // Check if "Ok" constructor exists, belongs to "Result",
        // AND the operand type structurally matches Result.
        if let Some(ok_info) = self.env.lookup_constructor("Ok") {
            if ok_info.type_name == "Result" && self.type_matches_adt(operand_type, "Result") {
                let success_type = self.extract_try_success_type(
                    operand_type,
                    &ok_info.type_name,
                    "Ok",
                    ok_info.index,
                )?;
                return Ok((TryKind::Result, success_type));
            }
        }

        // Check if "Some" constructor exists, belongs to "Option",
        // AND the operand type structurally matches Option.
        if let Some(some_info) = self.env.lookup_constructor("Some") {
            if some_info.type_name == "Option" && self.type_matches_adt(operand_type, "Option") {
                let success_type = self.extract_try_success_type(
                    operand_type,
                    &some_info.type_name,
                    "Some",
                    some_info.index,
                )?;
                return Ok((TryKind::Option, success_type));
            }
        }

        Err(ElabError::new(
            span,
            ElabErrorKind::TryOnNonTryType(format!("{}", operand_type)),
        ))
    }

    /// Extract the success type (T) from a Result<T,E> or Option<T> encoding.
    ///
    /// For 2-constructor ADTs with Sum encoding:
    /// - Alphabetical ordering: Err=0/inl, Ok=1/inr for Result; None=0/inl, Some=1/inr for Option
    /// - The Sum type is `Sum(err_payload, ok_payload)` for Result
    /// - The ok_payload IS the success type T
    fn extract_try_success_type(
        &self,
        operand_type: &Type,
        type_name: &str,
        success_ctor: &str,
        success_index: usize,
    ) -> ElabResult<Type> {
        let unfolded = unfold_mu(operand_type);

        // Navigate Sum to find the success constructor's type
        // For 2-ctor ADTs: Sum(ctor_0_payload, ctor_1_payload)
        // success_index tells us which side
        match &unfolded {
            Type::Sum(left, right) => {
                if success_index == 0 {
                    Ok((**left).clone())
                } else {
                    Ok((**right).clone())
                }
            }
            _ => {
                // Might be a non-recursive 2-ctor type without Mu wrapping
                // Try to look it up via the type definition
                self.extract_success_type_from_typedef(type_name, success_ctor, operand_type)
            }
        }
    }

    /// Fallback: extract success type from the type definition directly.
    fn extract_success_type_from_typedef(
        &self,
        type_name: &str,
        success_ctor: &str,
        operand_type: &Type,
    ) -> ElabResult<Type> {
        let type_def = self.env.lookup_type(type_name).ok_or_else(|| {
            ElabError::new(
                Span::new(0, 0),
                ElabErrorKind::UndefinedType(type_name.to_string()),
            )
        })?;

        if let TypeDefKind::ADT(ctors) = &type_def.kind {
            for ctor in ctors {
                if ctor.name == success_ctor {
                    // Single-field constructor: the field type is the success type
                    if ctor.fields.len() == 1 {
                        let mut field_type = ctor.fields[0].clone();
                        // Substitute type parameters if the operand is parameterized
                        if let Some(type_args) = self.extract_type_args(operand_type, type_name) {
                            for (i, param) in type_def.params.iter().enumerate() {
                                if let Some(arg) = type_args.get(i) {
                                    field_type = field_type.substitute(param, arg);
                                }
                            }
                        }
                        return Ok(field_type);
                    }
                }
            }
        }

        Err(ElabError::new(
            Span::new(0, 0),
            ElabErrorKind::Other(format!("cannot extract success type from `{}`", type_name)),
        ))
    }

    /// Try to extract type arguments from an operand type.
    fn extract_type_args(&self, ty: &Type, _type_name: &str) -> Option<Vec<Type>> {
        match ty {
            Type::App(_, args) => Some(args.clone()),
            _ => None,
        }
    }

    /// Check that the enclosing function's return type is compatible with `?`.
    fn check_try_return_type(
        &self,
        try_kind: &TryKind,
        operand_type: &Type,
        ret_ty: &Type,
        span: Span,
    ) -> ElabResult<()> {
        match try_kind {
            TryKind::Result => {
                // Return type must also be a Result whose error type matches
                if !self.is_result_type(ret_ty) {
                    return Err(ElabError::new(
                        span,
                        ElabErrorKind::TryReturnMismatch {
                            operand_type: format!("{}", operand_type),
                            return_type: format!("{}", ret_ty),
                        },
                    ));
                }
                // Check error types match
                let operand_err = self.extract_error_type(operand_type);
                let ret_err = self.extract_error_type(ret_ty);
                if let (Some(op_e), Some(ret_e)) = (&operand_err, &ret_err) {
                    if !self.types_equal(op_e, ret_e) {
                        return Err(ElabError::new(
                            span,
                            ElabErrorKind::TryReturnMismatch {
                                operand_type: format!("{}", operand_type),
                                return_type: format!("{}", ret_ty),
                            },
                        ));
                    }
                }
                Ok(())
            }
            TryKind::Option => {
                // Return type must also be an Option
                if !self.is_option_type(ret_ty) {
                    return Err(ElabError::new(
                        span,
                        ElabErrorKind::TryReturnMismatch {
                            operand_type: format!("{}", operand_type),
                            return_type: format!("{}", ret_ty),
                        },
                    ));
                }
                Ok(())
            }
        }
    }

    /// Check if a type is a Result type (has Ok/Err constructors).
    fn is_result_type(&self, ty: &Type) -> bool {
        if let Some(ok_info) = self.env.lookup_constructor("Ok") {
            if ok_info.type_name == "Result" {
                // Check if the type structurally matches a Result encoding
                return self.type_matches_adt(ty, "Result");
            }
        }
        false
    }

    /// Check if a type is an Option type (has Some/None constructors).
    fn is_option_type(&self, ty: &Type) -> bool {
        if let Some(some_info) = self.env.lookup_constructor("Some") {
            if some_info.type_name == "Option" {
                return self.type_matches_adt(ty, "Option");
            }
        }
        false
    }

    /// Check if a type matches a named ADT (by structure or name).
    ///
    /// For recursive ADTs: `Mu(α_Name, ...)` — check the Mu variable name.
    /// For non-recursive 2-ctor ADTs: bare `Sum(_, _)` — verify via type definition.
    pub(in crate::elaborate) fn type_matches_adt(&self, ty: &Type, adt_name: &str) -> bool {
        match ty {
            Type::Mu(var, _) => {
                let name = var.strip_prefix("α_").unwrap_or(var);
                name == adt_name
            }
            Type::App(name, _) => name == adt_name,
            // Non-recursive 2-ctor ADTs are encoded as bare Sum.
            // Verify the named ADT actually exists and has exactly 2 constructors.
            Type::Sum(_, _) => self
                .env
                .lookup_type(adt_name)
                .map(|td| matches!(&td.kind, TypeDefKind::ADT(ctors) if ctors.len() == 2))
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Extract the error type (E) from a Result<T, E>.
    /// Uses the Err constructor's index to determine which side of the Sum holds E.
    fn extract_error_type(&self, ty: &Type) -> Option<Type> {
        let err_index = self.env.lookup_constructor("Err").map(|info| info.index)?;
        let unfolded = unfold_mu(ty);
        match unfolded {
            Type::Sum(left, right) => {
                if err_index == 0 {
                    Some(*left)
                } else {
                    Some(*right)
                }
            }
            _ => None,
        }
    }

    /// Build the desugared Core term for `?`.
    ///
    /// For Result: case on the operand, propagate Err via early return, unwrap Ok.
    /// For Option: case on the operand, propagate None via early return, unwrap Some.
    ///
    /// Constructor index determines which side of the Sum is error/none vs success.
    fn build_try_desugaring(
        &mut self,
        try_kind: &TryKind,
        operand_term: Term,
        operand_type: &Type,
        ret_ty: &Type,
        _span: Span,
    ) -> ElabResult<Term> {
        let (scrutinee, sum_type) = if matches!(operand_type, Type::Mu(_, _)) {
            let unfolded_ty = unfold_mu(operand_type);
            (
                Term::unfold(operand_type.clone(), operand_term),
                unfolded_ty,
            )
        } else {
            (operand_term, operand_type.clone())
        };

        let err_var = "__try_err".to_string();
        let ok_var = "__try_ok".to_string();

        let error_index = match try_kind {
            TryKind::Result => self
                .env
                .lookup_constructor("Err")
                .map(|i| i.index)
                .unwrap_or(0),
            TryKind::Option => self
                .env
                .lookup_constructor("None")
                .map(|i| i.index)
                .unwrap_or(0),
        };

        let error_body = match try_kind {
            TryKind::Result => {
                let ret_sum = unfold_mu(ret_ty);
                let err_ref = Term::Var(err_var.clone());
                let err_inject = if error_index == 0 {
                    Term::Inl(ret_sum, Box::new(err_ref))
                } else {
                    Term::Inr(ret_sum, Box::new(err_ref))
                };
                let err_value = if matches!(ret_ty, Type::Mu(_, _)) {
                    Term::fold(ret_ty.clone(), err_inject)
                } else {
                    err_inject
                };
                Term::early_return(err_value)
            }
            TryKind::Option => {
                let ret_sum = unfold_mu(ret_ty);
                let none_value = if error_index == 0 {
                    Term::Inl(ret_sum, Box::new(Term::Unit))
                } else {
                    Term::Inr(ret_sum, Box::new(Term::Unit))
                };
                let none_folded = if matches!(ret_ty, Type::Mu(_, _)) {
                    Term::fold(ret_ty.clone(), none_value)
                } else {
                    none_value
                };
                Term::early_return(none_folded)
            }
        };

        let success_body = Term::Var(ok_var.clone());

        // case arms: left = index 0, right = index 1
        // If error constructor is index 0: left = error, right = success
        // If error constructor is index 1: left = success, right = error
        if error_index == 0 {
            Ok(Term::case(
                scrutinee,
                err_var,
                error_body,
                ok_var,
                success_body,
            ))
        } else {
            Ok(Term::case(
                scrutinee,
                ok_var,
                success_body,
                err_var,
                error_body,
            ))
        }
    }
}
