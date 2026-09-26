//! Build the strict-positivity engine's injected input (ADR 7.8.26e D1/D4).
//!
//! The rule must see imported and cache-reconstructed types, which never pass
//! through `TypeExpr` — so the input is the *elaborated* `Type`, not the
//! surface AST. Records are normalized into a single implicit constructor named
//! after the type, so the engine needs no second code path for them.
//!
//! Two entry points feed one [`InputBuilder`], so the elaborator gate
//! ([`from_env_types`], reading `env.types` mid-collection) and the report-only
//! tool ([`from_project`], reading a fully elaborated `ProjectOutput`) cannot
//! classify a definition differently.

use std::collections::{BTreeMap, BTreeSet};

use tungsten_core::types::positivity::{PositivityCtor, PositivityDef, PositivityDefs};
use tungsten_core::Type;

use crate::driver::ProjectOutput;
use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
use crate::span::Span;

/// Where a violation in a given type name should be reported.
pub type SpanIndex = BTreeMap<String, Span>;

/// Split type definitions into the engine's `defs`/`aliases`/`stubs` inputs
/// plus the span index the diagnostic needs.
#[derive(Default)]
struct InputBuilder {
    defs: BTreeMap<String, PositivityDef>,
    aliases: BTreeMap<String, (Vec<String>, Type)>,
    stubs: BTreeSet<String>,
    spans: SpanIndex,
}

impl InputBuilder {
    fn push(&mut self, name: &str, params: &[String], kind: &TypeDefKind, span: Span) {
        self.spans.insert(name.to_string(), span);
        match kind {
            TypeDefKind::ADT(constructors) => self.push_adt(name, params, constructors),
            TypeDefKind::Record(fields) => self.push_record(name, params, fields),
            TypeDefKind::Alias(body) => self.push_alias(name, params, body),
            // A stub's field types are lossy, so checking one is vacuous rather
            // than conservative — it is skipped, not doubted (D5).
            TypeDefKind::Stub => {
                self.stubs.insert(name.to_string());
            }
        }
    }

    fn push_adt(&mut self, name: &str, params: &[String], constructors: &[Constructor]) {
        self.defs.insert(
            name.to_string(),
            PositivityDef {
                params: params.to_vec(),
                ctors: constructors
                    .iter()
                    .map(|ctor| PositivityCtor::positional(ctor.name.clone(), ctor.fields.clone()))
                    .collect(),
                is_record: false,
            },
        );
    }

    fn push_record(&mut self, name: &str, params: &[String], fields: &[(String, Type)]) {
        self.defs.insert(
            name.to_string(),
            PositivityDef {
                params: params.to_vec(),
                ctors: vec![PositivityCtor::record(name.to_string(), fields.to_vec())],
                is_record: true,
            },
        );
    }

    fn push_alias(&mut self, name: &str, params: &[String], body: &Type) {
        self.aliases
            .insert(name.to_string(), (params.to_vec(), body.clone()));
    }

    fn finish(self) -> (PositivityDefs, SpanIndex) {
        (
            PositivityDefs::new(self.defs, &self.aliases, self.stubs),
            self.spans,
        )
    }
}

/// Engine input from the elaborator's live environment (the gate's path).
pub fn from_env_types<'a>(
    types: impl Iterator<Item = (&'a String, &'a TypeDef)>,
) -> (PositivityDefs, SpanIndex) {
    let mut builder = InputBuilder::default();
    for (name, type_def) in types {
        builder.push(name, &type_def.params, &type_def.kind, type_def.span);
    }
    builder.finish()
}

/// Engine input from a fully elaborated project (the report-only tool's path).
///
/// `ProjectOutput` carries no per-type spans and no stub set — by that point
/// residual stubs are their own finding (`doctor check type integrity
/// type-stubs`), so every
/// name that survives is either an ADT, a record or an alias.
pub fn from_project(project: &ProjectOutput) -> (PositivityDefs, SpanIndex) {
    let mut builder = InputBuilder::default();
    for (name, (params, constructors)) in &project.adt_types {
        builder.spans.insert(name.clone(), Span::default());
        builder.push_adt(name, params, constructors);
    }
    for (name, fields) in &project.record_types {
        builder.spans.insert(name.clone(), Span::default());
        builder.push_record(name, &[], fields);
    }
    for (name, (params, body)) in &project.type_aliases {
        builder.spans.insert(name.clone(), Span::default());
        builder.push_alias(name, params, body);
    }
    builder.finish()
}
