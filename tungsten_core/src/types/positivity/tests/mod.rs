//! Unit tests for the strict-positivity engine (ADR 7.8.26e §6).
//!
//! Split along the seam the engine itself has:
//!
//! - [`rule`] — what the walker *decides* about an occurrence: which positions
//!   are forbidden, how the strictness lattice behaves, how the `@`-prefix is
//!   resolved, and which types are accepted.
//! - [`parameters`] — D2's other half: the parameter-strictness fixpoint, the
//!   three-way argument dispatch it feeds, and the inherited-through chain.
//! - [`input`] — the layer *beneath* the rule: what [`PositivityDefs`] supplies
//!   to the walker (alias expansion, stub vs unknown classification, poison),
//!   and what the SCC collector must hand the caller so the group it checks is
//!   the true one.
//!
//! Every fixture is a hand-built [`PositivityDefs`], so these tests are pure
//! functions over injected data — no elaborator, no LLVM, no filesystem.

mod input;
mod parameters;
mod rule;

use std::collections::{BTreeMap, BTreeSet};

use crate::types::Type;

use super::*;

/// `type <name><params> = <ctor>(fields...)` as engine input.
pub(super) fn adt(
    name: &str,
    params: &[&str],
    ctors: Vec<(&str, Vec<Type>)>,
) -> (String, PositivityDef) {
    (
        name.to_string(),
        PositivityDef {
            params: params.iter().map(|p| (*p).to_string()).collect(),
            ctors: ctors
                .into_iter()
                .map(|(cname, fields)| PositivityCtor::positional(cname, fields))
                .collect(),
            is_record: false,
        },
    )
}

/// `type <name> = { field: ty, ... }` as engine input.
pub(super) fn record(name: &str, fields: Vec<(&str, Type)>) -> (String, PositivityDef) {
    (
        name.to_string(),
        PositivityDef {
            params: Vec::new(),
            ctors: vec![PositivityCtor::record(
                name,
                fields
                    .into_iter()
                    .map(|(f, ty)| (f.to_string(), ty))
                    .collect(),
            )],
            is_record: true,
        },
    )
}

pub(super) fn env(defs: Vec<(String, PositivityDef)>) -> PositivityDefs {
    PositivityDefs::new(
        defs.into_iter().collect(),
        &BTreeMap::new(),
        BTreeSet::new(),
    )
}

pub(super) fn env_with_aliases(
    defs: Vec<(String, PositivityDef)>,
    aliases: Vec<(&str, Vec<&str>, Type)>,
) -> PositivityDefs {
    let aliases: BTreeMap<String, (Vec<String>, Type)> = aliases
        .into_iter()
        .map(|(name, params, body)| {
            (
                name.to_string(),
                (params.iter().map(|p| (*p).to_string()).collect(), body),
            )
        })
        .collect();
    PositivityDefs::new(defs.into_iter().collect(), &aliases, BTreeSet::new())
}

pub(super) fn group(members: &[&str]) -> BTreeSet<String> {
    members.iter().map(|m| (*m).to_string()).collect()
}

/// Run the whole pipeline: fixpoint, then check one group.
pub(super) fn violations(defs: &PositivityDefs, members: &[&str]) -> Vec<PositivityViolation> {
    let occs = param_occurrences(defs);
    check_strict_positivity(&group(members), defs, &occs)
}

pub(super) fn tv(name: &str) -> Type {
    Type::TyVar(name.to_string())
}
