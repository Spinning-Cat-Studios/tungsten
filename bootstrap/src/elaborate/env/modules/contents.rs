//! Module contents and path resolution error types.

use std::collections::HashMap;

use crate::ast::Visibility;

use crate::elaborate::env::ImportInfo;

use super::path::ModulePath;

/// Contents of a module (types, values, constructors, and imports).
#[derive(Debug, Clone, Default)]
pub struct ModuleContents {
    /// Type names defined in this module (with visibility)
    pub types: Vec<String>,
    /// Visibility of each type (by name)
    pub type_visibility: HashMap<String, Visibility>,
    /// Type parameter counts for generic types (by name).
    /// Used when creating stub TypeDefs to preserve arity information.
    /// See ADR 30.1.26.1 for details on this fix.
    pub type_param_counts: HashMap<String, usize>,
    /// Value names defined in this module
    pub values: Vec<String>,
    /// Visibility of each value (by name)
    pub value_visibility: HashMap<String, Visibility>,
    /// Constructor names defined in this module
    pub constructors: Vec<String>,
    /// Visibility of each constructor (by name, inherits from parent type)
    pub constructor_visibility: HashMap<String, Visibility>,
    /// Constructor details: name → (type_name, index, arity).
    /// Used by per-module elaboration (ADR 5.5.26b) to create constructor stubs
    /// so cross-branch imports can resolve before the defining module is elaborated.
    pub constructor_details: HashMap<String, ConstructorStubDetail>,
    /// Imported types: local name → import info
    pub imported_types: HashMap<String, ImportInfo>,
    /// Imported values: local name → import info
    pub imported_values: HashMap<String, ImportInfo>,
    /// Imported constructors: local name → import info
    pub imported_constructors: HashMap<String, ImportInfo>,
}

/// Pre-elaboration constructor details from the parsed AST (ADR 5.5.26b).
#[derive(Debug, Clone)]
pub struct ConstructorStubDetail {
    /// Name of the parent type.
    pub type_name: String,
    /// Index of this constructor in the ADT variant list.
    pub index: usize,
    /// Number of fields (positional arguments).
    pub arity: usize,
}

/// Error during path resolution.
#[derive(Debug, Clone)]
pub enum PathResolutionError {
    /// Module not found in the registry
    ModuleNotFound(ModulePath),
    /// Item not found in the specified module
    ItemNotFound { module: ModulePath, item: String },
}
