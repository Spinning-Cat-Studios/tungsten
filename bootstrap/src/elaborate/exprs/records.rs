//! Record type and record literal elaboration.
//!
//! Handles:
//! - `elab_record_literal` - record literal construction
//! - `elab_field_access` - field access elaboration

use std::collections::HashMap;

use crate::ast::{Expr, Ident};
use crate::span::Span;
use tungsten_core::{Term, Type};

use crate::elaborate::env::TypeDefKind;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate a record literal against an expected type.
    ///
    /// Record literals require an expected type since we can't infer the record type
    /// purely from field names. Supports optional spread syntax: `{ ...base, field: value }`
    ///
    /// Spread desugars to:
    /// ```text
    /// { ...base, f1: e1, f2: e2 }
    /// =>
    /// let tmp = base in { f1: e1, f2: e2, f3: tmp.f3, ..., fn: tmp.fn }
    /// ```
    pub(super) fn elab_record_literal(
        &mut self,
        spread: Option<&Expr>,
        fields: &[(Ident, Expr)],
        expected: &Type,
        span: Span,
    ) -> ElabResult<Term> {
        // 1. Resolve the expected type to a record type definition
        let (type_name, record_fields) = self.resolve_record_type(expected, span)?;

        // 2. Build a map of provided fields
        let mut field_map: HashMap<&str, &Expr> = HashMap::new();
        for (ident, expr) in fields {
            if field_map.insert(&ident.name, expr).is_some() {
                return Err(ElabError::new(
                    ident.span,
                    ElabErrorKind::Other(format!("duplicate field `{}`", ident.name)),
                ));
            }
        }

        // 3. Check for extra fields (unknown fields not in the record type)
        for (name, _) in field_map.iter() {
            if !record_fields.iter().any(|(f, _)| f == *name) {
                return Err(ElabError::new(
                    span,
                    ElabErrorKind::Other(format!(
                        "unknown field `{}` in record of type `{}`",
                        name, type_name
                    )),
                ));
            }
        }

        // 4. Handle spread if present
        if let Some(spread_expr) = spread {
            // Elaborate spread and verify it has the expected record type
            let spread_term = self.check(spread_expr, expected)?;

            // Build the record with spread: we need to wrap in a let binding
            // to ensure single evaluation, then use field accesses for missing fields
            self.elab_record_with_spread(spread_term, &field_map, &record_fields, expected)
        } else {
            // No spread: all fields must be explicitly provided
            self.elab_record_without_spread(&field_map, &type_name, &record_fields, span)
        }
    }

    /// Elaborate a record literal without spread (all fields must be explicit).
    fn elab_record_without_spread(
        &mut self,
        field_map: &HashMap<&str, &Expr>,
        type_name: &str,
        record_fields: &[(String, Type)],
        span: Span,
    ) -> ElabResult<Term> {
        let mut elaborated = Vec::new();
        let mut field_map = field_map.clone();

        for (field_name, field_ty) in record_fields {
            match field_map.remove(field_name.as_str()) {
                Some(expr) => {
                    let term = self.check(expr, field_ty)?;
                    elaborated.push(term);
                }
                None => {
                    return Err(ElabError::new(
                        span,
                        ElabErrorKind::Other(format!(
                            "missing field `{}` in record of type `{}`",
                            field_name, type_name
                        )),
                    ));
                }
            }
        }

        Ok(self.build_nested_pair(elaborated))
    }

    /// Elaborate a record literal with spread.
    ///
    /// Desugars to: `let tmp = spread in { ... }` where missing fields
    /// are filled from `tmp.field`.
    fn elab_record_with_spread(
        &mut self,
        spread_term: Term,
        field_map: &HashMap<&str, &Expr>,
        record_fields: &[(String, Type)],
        expected: &Type,
    ) -> ElabResult<Term> {
        // Generate a fresh variable name for the spread binding
        let spread_var = self.fresh_var("spread");
        let total_fields = record_fields.len();

        // Build record value referencing the spread variable
        let mut elaborated = Vec::new();
        for (position, (field_name, field_ty)) in record_fields.iter().enumerate() {
            if let Some(expr) = field_map.get(field_name.as_str()) {
                // Field is explicitly provided
                let term = self.check(expr, field_ty)?;
                elaborated.push(term);
            } else {
                // Field comes from spread: project from the spread variable
                let spread_ref = Term::var(&spread_var);
                let projection = self.build_projection(spread_ref, position, total_fields);
                elaborated.push(projection);
            }
        }

        let record_body = self.build_nested_pair(elaborated);

        // Wrap in let: let spread_var : T = spread_term in record_body
        Ok(Term::let_in(
            &spread_var,
            expected.clone(),
            spread_term,
            record_body,
        ))
    }

    /// Elaborate a field access expression.
    pub(super) fn elab_field_access(
        &mut self,
        base: &Expr,
        field: &Ident,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // 1. Infer base expression type
        let (base_term, base_ty) = self.infer(base)?;

        // 2. Resolve record type
        let (_type_name, record_fields) = self.resolve_record_type(&base_ty, span)?;

        // 3. Find field position
        let (position, field_ty) = record_fields
            .iter()
            .enumerate()
            .find(|(_, (name, _))| name == &field.name)
            .map(|(i, (_, ty))| (i, ty.clone()))
            .ok_or_else(|| {
                ElabError::new(
                    field.span,
                    ElabErrorKind::Other(format!("unknown field `{}`", field.name)),
                )
            })?;

        // 4. Build projection chain
        let total = record_fields.len();
        let projection = self.build_projection(base_term, position, total);

        Ok((projection, field_ty))
    }

    /// Resolve a type to a record type definition.
    ///
    /// Returns the type name and the ordered list of (field_name, field_type).
    ///
    /// Record types are represented as TyVar("RecordName") during elaboration,
    /// or as Type::App("RecordName", []) for cross-module references.
    /// This function looks up the record definition from the type name.
    ///
    /// # Cross-Module Type Handling (ADR 30.1.26 Category B Fix)
    ///
    /// Cross-module record types appear as `Type::App("RecordName", [])` instead
    /// of `Type::TyVar("RecordName")`. We handle both cases.
    fn resolve_record_type(
        &self,
        ty: &Type,
        span: Span,
    ) -> ElabResult<(String, Vec<(String, Type)>)> {
        // Extract the type name from either TyVar or App representation
        let type_name = match ty {
            // Local record types appear as TyVar("RecordName")
            Type::TyVar(name) => name.clone(),
            // Cross-module record types appear as App("RecordName", [])
            Type::App(name, args) if args.is_empty() => name.clone(),
            _ => {
                // Not a record type - produce a user-friendly error
                return Err(ElabError::new(
                    span,
                    ElabErrorKind::Other(format!("expected record type, found `{}`", ty)),
                ));
            }
        };

        // Look up the type definition and verify it's a record
        if let Some(type_def) = self.env.lookup_type(&type_name) {
            if let TypeDefKind::Record(fields) = &type_def.kind {
                return Ok((type_name, fields.clone()));
            }
        }

        // Type exists but is not a record, or type doesn't exist
        Err(ElabError::new(
            span,
            ElabErrorKind::Other(format!("expected record type, found `{}`", ty)),
        ))
    }

    /// Build a product type from record fields.
    ///
    /// `[(f1, T1), (f2, T2), (f3, T3)]` → `T1 × (T2 × T3)`
    fn build_product_type_from_fields(&self, fields: &[(String, Type)]) -> Type {
        assert!(!fields.is_empty());

        let types: Vec<Type> = fields.iter().map(|(_, t)| t.clone()).collect();

        if types.len() == 1 {
            return types.into_iter().next().unwrap();
        }

        // Right-nested: T1 × (T2 × (T3 × T4))
        let mut iter = types.into_iter().rev();
        let last = iter.next().unwrap();
        iter.fold(last, |acc, t| Type::product(t, acc))
    }

    /// Build a right-nested pair from a list of terms.
    ///
    /// `[a, b, c]` → `(a, (b, c))`
    fn build_nested_pair(&self, mut values: Vec<Term>) -> Term {
        assert!(!values.is_empty());

        if values.len() == 1 {
            return values.pop().unwrap();
        }

        let last = values.pop().unwrap();
        values
            .into_iter()
            .rev()
            .fold(last, |acc, v| Term::pair(v, acc))
    }

    /// Build a projection chain for field access.
    ///
    /// For a record `{ f0, f1, f2 }` stored as `(v0, (v1, v2))`:
    /// - Field 0: `fst(e)`
    /// - Field 1: `fst(snd(e))`
    /// - Field 2: `snd(snd(e))` (last field, no final fst)
    fn build_projection(&self, base: Term, position: usize, total_fields: usize) -> Term {
        let mut result = base;

        // Navigate to the right position
        for _ in 0..position {
            result = Term::snd(result);
        }

        // Extract the field (fst unless it's the last field)
        if position < total_fields - 1 {
            result = Term::fst(result);
        }

        result
    }
}
