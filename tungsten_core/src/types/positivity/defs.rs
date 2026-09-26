//! The injected data the strict-positivity engine runs over (ADR 7.8.26e D4),
//! plus D9's alias expansion.
//!
//! The engine takes no `Elaborator` reference and performs no environment
//! lookups: the caller hands it a name → (parameters, per-constructor field
//! types) map, the set of names whose bodies are lossy stubs, and the alias
//! table. Absence from the resulting map is *meaningful* (D5), not a lookup
//! failure.

use std::collections::{BTreeMap, BTreeSet};

use crate::types::Type;

/// Which field of a constructor an occurrence was found in.
///
/// A record is normalized into a single implicit constructor (D4), so the two
/// spellings are the only rendering difference records need.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FieldRef {
    /// Positional field of an ADT constructor.
    Index(usize),
    /// Named field of a record.
    Named(String),
}

impl std::fmt::Display for FieldRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldRef::Index(i) => write!(f, "field {i}"),
            FieldRef::Named(name) => write!(f, "field `{name}`"),
        }
    }
}

/// One constructor's positional/named fields.
///
/// A record contributes exactly one of these, named after the record type
/// itself, with its fields in declaration order.
#[derive(Debug, Clone)]
pub struct PositivityCtor {
    /// Constructor name; for a record, the record type's own name.
    pub name: String,
    /// Field reference (positional index or record field name) and field type.
    pub fields: Vec<(FieldRef, Type)>,
}

impl PositivityCtor {
    /// An ADT constructor from positional field types.
    #[must_use]
    pub fn positional(name: impl Into<String>, fields: Vec<Type>) -> Self {
        PositivityCtor {
            name: name.into(),
            fields: fields
                .into_iter()
                .enumerate()
                .map(|(i, ty)| (FieldRef::Index(i), ty))
                .collect(),
        }
    }

    /// The implicit single constructor of a record, from `(name, type)` fields.
    #[must_use]
    pub fn record(type_name: impl Into<String>, fields: Vec<(String, Type)>) -> Self {
        PositivityCtor {
            name: type_name.into(),
            fields: fields
                .into_iter()
                .map(|(fname, ty)| (FieldRef::Named(fname), ty))
                .collect(),
        }
    }
}

/// One checkable type definition: its parameters and its constructors.
#[derive(Debug, Clone)]
pub struct PositivityDef {
    /// Type parameters in declaration order. Load-bearing for `TyVar` role (c)
    /// — telling a parameter reference apart from a group-member reference.
    pub params: Vec<String>,
    /// Constructors (exactly one, implicit, for a record).
    pub ctors: Vec<PositivityCtor>,
    /// Whether this definition came from a record, for diagnostic rendering.
    pub is_record: bool,
}

/// The complete injected environment: alias-expanded definitions plus the
/// stub-name set.
#[derive(Debug, Clone, Default)]
pub struct PositivityDefs {
    defs: BTreeMap<String, PositivityDef>,
    stubs: BTreeSet<String>,
}

impl PositivityDefs {
    /// Build the environment, inlining every alias body into the field types
    /// that reference it (D9).
    ///
    /// `aliases` maps an alias name to `(parameters, body)`. Aliases are *not*
    /// nodes in the resulting map: a violation is always attributed to an
    /// ADT/record constructor field, never to an alias with no constructor to
    /// point at.
    #[must_use]
    pub fn new(
        defs: BTreeMap<String, PositivityDef>,
        aliases: &BTreeMap<String, (Vec<String>, Type)>,
        stubs: BTreeSet<String>,
    ) -> Self {
        let expanded = defs
            .into_iter()
            .map(|(name, def)| {
                let ctors = def
                    .ctors
                    .into_iter()
                    .map(|ctor| PositivityCtor {
                        name: ctor.name,
                        fields: ctor
                            .fields
                            .into_iter()
                            .map(|(field, ty)| {
                                let mut expander = AliasExpander {
                                    aliases,
                                    in_progress: Vec::new(),
                                };
                                (field, expander.expand(&ty))
                            })
                            .collect(),
                    })
                    .collect();
                (
                    name,
                    PositivityDef {
                        params: def.params,
                        ctors,
                        is_record: def.is_record,
                    },
                )
            })
            .collect();
        PositivityDefs {
            defs: expanded,
            stubs,
        }
    }

    /// Look up a definition by name. `None` is D5's "absent" case.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&PositivityDef> {
        self.defs.get(name)
    }

    /// Whether `name` is a lossy `TypeDefKind::Stub` — the one absence that is
    /// *skipped* rather than doubted (D5).
    #[must_use]
    pub fn is_stub(&self, name: &str) -> bool {
        self.stubs.contains(name)
    }

    /// Every checkable definition name, sorted.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.defs.keys()
    }

    /// Every checkable definition, sorted by name.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &PositivityDef)> {
        self.defs.iter()
    }

    /// Number of checkable definitions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// Whether there are no checkable definitions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

/// Inline alias bodies, substituting type arguments (D9).
///
/// Aliases are not nodes in the checked graph, so a violation is always
/// attributed to an ADT/record constructor field rather than to an alias with
/// no constructor to point at. Two malformed inputs bail to [`Type::Error`]
/// rather than to an unexpandable head — a cyclic alias and an arity mismatch.
/// An unexpandable head would fall into D5's absent-`defs` arm, produce
/// `Forbidden` arguments and stack an E0061 on top of an E0060-class malformed
/// alias; `Type::Error` is skipped by the walker, so the bail is silent by
/// construction.
struct AliasExpander<'a> {
    aliases: &'a BTreeMap<String, (Vec<String>, Type)>,
    /// The expansion stack — a name reached twice is a cycle, which is E0060
    /// `RecursiveAlias`'s job to diagnose, not ours.
    in_progress: Vec<String>,
}

impl AliasExpander<'_> {
    fn expand(&mut self, ty: &Type) -> Type {
        match ty {
            Type::TyVar(raw) => self.expand_tyvar(raw, ty),
            Type::App(name, args) => self.expand_app(name, args),
            // Every other variant is structural: rebuild it with expanded
            // children. `Eq` witness *terms* are left alone — an alias name
            // surviving there is handled by the walker's absent-`defs` arm, and
            // that position is already forbidden.
            _ => ty.map_children(|child| self.expand(child)),
        }
    }

    /// A bare name: only a *nullary* alias expands here.
    fn expand_tyvar(&mut self, raw: &str, original: &Type) -> Type {
        let name = raw.strip_prefix('@').unwrap_or(raw);
        let Some((params, body)) = self.aliases.get(name).cloned() else {
            return original.clone();
        };
        if !params.is_empty() {
            return Type::Error; // arity mismatch: not this check's error to raise
        }
        self.instantiate(name, &body, &params, &[])
    }

    fn expand_app(&mut self, name: &str, args: &[Type]) -> Type {
        let args: Vec<Type> = args.iter().map(|arg| self.expand(arg)).collect();
        let Some((params, body)) = self.aliases.get(name).cloned() else {
            return Type::App(name.to_string(), args);
        };
        if params.len() != args.len() {
            return Type::Error;
        }
        self.instantiate(name, &body, &params, &args)
    }

    /// Substitute `args` for `params` in an alias body, then keep expanding.
    fn instantiate(&mut self, name: &str, body: &Type, params: &[String], args: &[Type]) -> Type {
        if self.in_progress.iter().any(|n| n == name) {
            return Type::Error;
        }
        let mut substituted = body.clone();
        for (param, arg) in params.iter().zip(args) {
            substituted = substituted.substitute(param, arg);
        }
        self.in_progress.push(name.to_string());
        let result = self.expand(&substituted);
        self.in_progress.pop();
        result
    }
}
