//! Pass 1: Definition Collection
//!
//! Collects type and value signatures from top-level items so that
//! items can refer to each other (forward references).

use crate::ast;

use tungsten_core::Type;

use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind, ValueDef};
use crate::elaborate::error::ElabError;
use crate::elaborate::ElabResult;
use crate::elaborate::Elaborator;

impl<'a> Elaborator<'a> {
    pub(super) fn collect_function(&mut self, func: &ast::FunctionDef) -> ElabResult<()> {
        // Check for duplicate
        if self.env.has_value(&func.name.name) {
            return Err(ElabError::duplicate(func.name.span, &func.name.name));
        }

        // Build function type
        let ty = self.build_function_type(func)?;

        self.env.define_value(ValueDef {
            name: func.name.name.clone(),
            ty,
            visibility: func.visibility,
            span: func.span,
        });

        Ok(())
    }

    pub(super) fn collect_type_def(&mut self, type_def: &ast::TypeDef) -> ElabResult<()> {
        // Check for duplicate (but allow replacing stubs from Phase 1a)
        if let Some(existing) = self.env.lookup_type(&type_def.name.name) {
            let is_stub = matches!(existing.kind, TypeDefKind::Stub);
            if !is_stub {
                return Err(ElabError::duplicate(
                    type_def.name.span,
                    &type_def.name.name,
                ));
            }
        }

        // Extract type parameters
        let params: Vec<String> = type_def
            .type_params
            .iter()
            .map(|p| p.name.name.clone())
            .collect();

        // Build type variables scope for elaborating variants/fields
        // Also add the type name itself as a type variable for recursive references
        self.env.push_type_var(type_def.name.name.clone());
        for param in &params {
            self.env.push_type_var(param.clone());
        }

        let type_def_kind = match &type_def.body {
            ast::TypeBody::Sum(variants) => {
                // Elaborate sum type variants
                let mut constructors = Vec::new();
                for (index, variant) in variants.iter().enumerate() {
                    let fields: Result<Vec<Type>, _> = variant
                        .fields
                        .iter()
                        .map(|f| self.elab_type(&f.ty))
                        .collect();
                    let fields = fields?;

                    constructors.push(Constructor {
                        name: variant.name.name.clone(),
                        fields,
                        index,
                        span: variant.span,
                    });
                }
                TypeDefKind::ADT(constructors)
            }
            ast::TypeBody::Record(record_fields) => {
                // Elaborate record type fields
                let mut fields = Vec::new();
                for field in record_fields {
                    let ty = self.elab_type(&field.ty)?;
                    fields.push((field.name.name.clone(), ty));
                }
                TypeDefKind::Record(fields)
            }
        };

        // Pop type variables
        for _ in &params {
            self.env.pop_type_var();
        }
        self.env.pop_type_var(); // Pop the self-reference type var

        self.env.define_type(TypeDef {
            name: type_def.name.name.clone(),
            params,
            kind: type_def_kind,
            visibility: type_def.visibility,
            span: type_def.span,
            defining_module: None,
            encoded_type: None,
        });

        Ok(())
    }

    pub(super) fn collect_type_alias(&mut self, alias: &ast::TypeAlias) -> ElabResult<()> {
        // Check for duplicate (but allow replacing stubs from Phase 1a)
        if let Some(existing) = self.env.lookup_type(&alias.name.name) {
            if !matches!(existing.kind, TypeDefKind::Stub) {
                return Err(ElabError::duplicate(alias.name.span, &alias.name.name));
            }
        }

        // Extract type parameters
        let params: Vec<String> = alias
            .type_params
            .iter()
            .map(|p| p.name.name.clone())
            .collect();

        // Push type variables for elaboration
        for param in &params {
            self.env.push_type_var(param.clone());
        }

        // Elaborate the aliased type
        let ty = self.elab_type(&alias.ty)?;

        // Pop type variables
        for _ in &params {
            self.env.pop_type_var();
        }

        self.env.define_type(TypeDef {
            name: alias.name.name.clone(),
            params,
            kind: TypeDefKind::Alias(ty),
            visibility: alias.visibility,
            span: alias.span,
            defining_module: None,
            encoded_type: None,
        });

        Ok(())
    }

    pub(super) fn collect_theorem(&mut self, thm: &ast::TheoremDef) -> ElabResult<()> {
        // Check for duplicate
        if self.env.has_value(&thm.name.name) {
            return Err(ElabError::duplicate(thm.name.span, &thm.name.name));
        }

        // Build theorem type: ∀ type_params. (param_types) → prop
        let ty = self.build_theorem_type(thm)?;

        self.env.define_value(ValueDef {
            name: thm.name.name.clone(),
            ty,
            visibility: thm.visibility,
            span: thm.span,
        });

        Ok(())
    }

    pub(super) fn collect_axiom(&mut self, axiom: &ast::AxiomDef) -> ElabResult<()> {
        // Check for duplicate
        if self.env.has_value(&axiom.name.name) {
            return Err(ElabError::duplicate(axiom.name.span, &axiom.name.name));
        }

        // Build axiom type (same as theorem)
        let ty = self.build_axiom_type(axiom)?;

        self.env.define_value(ValueDef {
            name: axiom.name.name.clone(),
            ty,
            visibility: axiom.visibility,
            span: axiom.span,
        });

        Ok(())
    }
}
