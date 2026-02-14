//! Type, value, and constructor definitions for the environment.

use serde::{Deserialize, Serialize};

use crate::ast::Visibility;
use crate::span::Span;
use tungsten_core::Type;

use super::ModulePath;

/// A type definition (ADT or type alias).
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
    /// The module where this type is canonically defined.
    /// For stubs created from imports, this points to the original defining module.
    /// None for types defined in the current compilation unit.
    #[serde(skip)]
    pub defining_module: Option<ModulePath>,
    /// Cached encoded type (for non-parameterized types).
    /// Records → product encoding, ADTs → sum/μ encoding.
    /// None if not yet computed or type has parameters.
    #[serde(skip)]
    pub encoded_type: Option<Type>,
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
    /// Placeholder stub (used during Phase 1a before body elaboration)
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
    /// Source span
    pub span: Span,
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
    /// The module where this constructor's type is canonically defined.
    /// Used for canonical type lookup when the constructor is imported.
    #[serde(skip)]
    pub defining_module: Option<ModulePath>,
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
