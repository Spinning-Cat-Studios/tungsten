//! Type, value, and constructor definitions for the environment.

use serde::{Deserialize, Serialize};

use crate::ast::Visibility;
use crate::span::Span;
use tungsten_core::Type;

use super::ModulePath;

/// A type definition (ADT or type alias).
///
/// **Elaboration** representation (`Type`/`TypeDefKind`).
/// See also: `ast::items::TypeDef` (AST/surface syntax representation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDef {
    /// Name of the type
    pub name: String,
    /// Type parameters (e.g., T in `type Option<T>`)
    pub params: Vec<String>,
    /// Kind of type definition
    pub kind: TypeDefKind,
    /// Visibility of this type
    pub visibility: Visibility,
    /// Source span
    pub span: Span,
    /// The module a type was *imported* from, or `None` when it is defined in
    /// the current compilation unit.
    ///
    /// **Convention (ADR 5.5.26c):** `Some(path)` marks an *import stub* whose
    /// real definition lives in `path`; a type collected from the current
    /// unit's own source has `defining_module: None`. So `None` means
    /// "locally defined" and `Some` means "placeholder for an import" — read it
    /// as import-provenance, not "where it's defined" (which the name suggests).
    /// [`Self::is_overwritable_by_collection`] relies on this so per-module
    /// re-collection may replace a local placeholder, but never an import.
    #[serde(skip)]
    pub defining_module: Option<ModulePath>,
    /// Cached encoded type (for non-parameterized types).
    /// Records → product encoding, ADTs → sum/μ encoding.
    /// None if not yet computed or type has parameters.
    #[serde(skip)]
    pub encoded_type: Option<Type>,
    /// Per-field visibility for record types (parallel to record fields in
    /// `TypeDefKind::Record`). Empty for non-record types.
    /// `None` per-entry = inherit parent type visibility.
    #[serde(default)]
    pub field_visibilities: Vec<Option<Visibility>>,
}

impl TypeDef {
    /// Whether a fresh collection may overwrite this existing definition
    /// without a duplicate-definition error (ADR 5.5.26c).
    ///
    /// An existing entry is overwritable when it is a placeholder or a
    /// re-collectible local, rather than a finalized *import* — any of:
    /// - a `Stub` (registered by Type-Name Registration / a workspace sibling's import), or
    /// - not yet encoded (`encoded_type.is_none()` — a Stub Registration placeholder ADT), or
    /// - locally defined (`defining_module.is_none()` — see that field's note),
    ///   which per-module re-collection (Signature Collection → Body Elaboration) must be able to
    ///   replace with the module's own fully-elaborated definition.
    ///
    /// An imported type (`defining_module.is_some()`) is NOT overwritable by
    /// this clause, so a module cannot redefine a name it merely imports.
    /// Shared by the collection-pass duplicate checks (`register_type_name`,
    /// `collect_type_def`, `collect_type_alias`) so the rule lives in one place.
    pub fn is_overwritable_by_collection(&self) -> bool {
        matches!(self.kind, TypeDefKind::Stub)
            || self.encoded_type.is_none()
            || self.defining_module.is_none()
    }

    /// Whether this definition is the residue of a failed type body (ADR
    /// 14.8.26g D3, ADR 15.8.26d): the producer kept the Type-Name
    /// Registration stub and set its encoding to `Type::Error`, so the fault
    /// was already reported at the type's own span.
    ///
    /// Every consumer that takes the definition apart to *build* or *take
    /// apart* a value (a record literal, a constructor call, a match) asks
    /// this first and passes the poison through instead of re-diagnosing
    /// the same fault at every use site. Deliberately exact — `Some(Error)`,
    /// not "contains poison" — so a healthy type whose encoding merely
    /// *mentions* a poisoned one keeps its own field checks.
    #[must_use]
    pub fn is_poison(&self) -> bool {
        matches!(self.encoded_type, Some(Type::Error))
    }

    /// Whether the global Signature Collection pass exports this definition
    /// to the per-module walk (ADR 5.5.26c, 15.8.26d).
    ///
    /// Types come from Stub Registration, so a plain stub is withheld — it
    /// would clobber the placeholder that already carries the source-declared
    /// shape. A POISONED stub is the one exception: it must replace that
    /// placeholder, otherwise every dependent module keeps building against
    /// the placeholder's unresolved field types and re-diagnoses the fault
    /// the producer already reported (V5's 23 `E0010`s).
    #[must_use]
    pub fn is_signature_collection_export(&self) -> bool {
        !matches!(self.kind, TypeDefKind::Stub) || self.is_poison()
    }
}

#[cfg(test)]
impl TypeDef {
    /// Minimal test constructor with sensible defaults.
    /// `visibility: Public`, empty params/field_visibilities, no encoded type.
    pub fn test_stub(name: &str, kind: TypeDefKind) -> Self {
        Self {
            name: name.to_string(),
            params: vec![],
            kind,
            visibility: Visibility::Public,
            span: Span::default(),
            defining_module: None,
            encoded_type: None,
            field_visibilities: vec![],
        }
    }
}

/// The kind of a type definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeDefKind {
    /// Type alias: `type Foo = Bar`
    Alias(Type),
    /// Algebraic data type: `type Option<T> = None | Some(T)`
    ADT(Vec<Constructor>),
    /// Record type: `type Point = { x: Nat, y: Nat }`
    Record(Vec<(String, Type)>),
    /// Placeholder stub (used during Type-Name Registration before body elaboration)
    Stub,
}

/// A constructor of an ADT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constructor {
    /// Constructor name (e.g., "Some", "None")
    pub name: String,
    /// Field types (positional)
    pub fields: Vec<Type>,
    /// Index of this constructor in the ADT (for encoding as sum type)
    pub index: usize,
    /// Explicit visibility (None = inherit parent type visibility)
    pub visibility: Option<Visibility>,
    /// Source span
    pub span: Span,
}

#[cfg(test)]
impl Constructor {
    /// Minimal test constructor with sensible defaults.
    /// `visibility: None` (inherit parent), empty fields, default span.
    pub fn test_stub(name: &str, index: usize) -> Self {
        Self {
            name: name.to_string(),
            fields: vec![],
            index,
            visibility: None,
            span: Span::default(),
        }
    }

    /// Test constructor with specified fields.
    pub fn test_with_fields(name: &str, index: usize, fields: Vec<Type>) -> Self {
        Self {
            name: name.to_string(),
            fields,
            index,
            visibility: None,
            span: Span::default(),
        }
    }
}

/// Information about a constructor, including its parent type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstructorInfo {
    /// Name of the parent type
    pub type_name: String,
    /// Index of this constructor in the parent type
    pub index: usize,
    /// Number of fields
    pub arity: usize,
    /// Explicit visibility (None = inherit parent type visibility)
    pub visibility: Option<Visibility>,
    /// The module where this constructor's type is canonically defined.
    /// Used for canonical type lookup when the constructor is imported.
    #[serde(skip)]
    pub defining_module: Option<ModulePath>,
}

#[cfg(test)]
impl ConstructorInfo {
    /// Minimal test constructor with sensible defaults.
    /// `visibility: None` (inherit parent), `defining_module: None`.
    pub fn test_stub(type_name: &str, index: usize, arity: usize) -> Self {
        Self {
            type_name: type_name.to_string(),
            index,
            arity,
            visibility: None,
            defining_module: None,
        }
    }
}

/// A value definition (function, theorem, or axiom).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueDef {
    /// Name of the value
    pub name: String,
    /// Type of the value
    pub ty: Type,
    /// Visibility of this value
    pub visibility: Visibility,
    /// Source span
    pub span: Span,
}

/// A local variable binding (in a let, lambda, or function parameter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalBinding {
    /// Variable name
    pub name: String,
    /// Type of the variable
    pub ty: Type,
    /// de Bruijn level (depth at binding time)
    pub level: usize,
}

/// The result of resolving a value name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResolvedValue {
    /// A local variable (de Bruijn index)
    Local(usize, Type),
    /// A global definition
    Global(String, Type),
    /// A constructor
    Constructor(ConstructorInfo),
}
