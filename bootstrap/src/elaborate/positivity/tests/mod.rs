//! Driver-level tests for the strict-positivity gate (ADR 7.8.26e §6).
//!
//! The lattice, fixpoint and walker are unit-tested in
//! `tungsten_core::types::positivity`. What is tested here is the *glue*, split
//! along the seam the driver has:
//!
//! - [`grouping`] — turning `env.types` into the engine's input and into the
//!   SCCs the walker needs: alias expansion, records as nodes, stub and unknown
//!   classification, the import/elab-cache seam, Tarjan depth.
//! - [`gate`] — what the elaborator does with a violation: the E0061 rendering,
//!   the span it is reported at, the hook itself, and the D3 cross-check.

mod gate;
mod grouping;
mod mirror_agreement;

use std::collections::BTreeMap;

use tungsten_core::Type;

use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};

use super::{analyze, from_env_types};

pub(super) fn tv(name: &str) -> Type {
    Type::TyVar(name.to_string())
}

pub(super) fn adt(name: &str, params: &[&str], ctors: Vec<(&str, Vec<Type>)>) -> (String, TypeDef) {
    let mut def = TypeDef::test_stub(
        name,
        TypeDefKind::ADT(
            ctors
                .into_iter()
                .enumerate()
                .map(|(index, (cname, fields))| Constructor::test_with_fields(cname, index, fields))
                .collect(),
        ),
    );
    def.params = params.iter().map(|p| (*p).to_string()).collect();
    (name.to_string(), def)
}

pub(super) fn record(name: &str, fields: Vec<(&str, Type)>) -> (String, TypeDef) {
    (
        name.to_string(),
        TypeDef::test_stub(
            name,
            TypeDefKind::Record(
                fields
                    .into_iter()
                    .map(|(f, ty)| (f.to_string(), ty))
                    .collect(),
            ),
        ),
    )
}

pub(super) fn alias(name: &str, params: &[&str], body: Type) -> (String, TypeDef) {
    let mut def = TypeDef::test_stub(name, TypeDefKind::Alias(body));
    def.params = params.iter().map(|p| (*p).to_string()).collect();
    (name.to_string(), def)
}

pub(super) fn run(types: Vec<(String, TypeDef)>) -> super::PositivityReport {
    let map: BTreeMap<String, TypeDef> = types.into_iter().collect();
    let (defs, spans) = from_env_types(map.iter());
    analyze(&defs, spans)
}
