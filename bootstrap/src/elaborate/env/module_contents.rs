//! Module contents and path resolution error types.

use std::collections::HashMap;

use crate::ast::Visibility;

use super::imports::ImportInfo;
use super::ModulePath;

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
    /// Imported types: local name → import info
    pub imported_types: HashMap<String, ImportInfo>,
    /// Imported values: local name → import info
    pub imported_values: HashMap<String, ImportInfo>,
    /// Imported constructors: local name → import info
    pub imported_constructors: HashMap<String, ImportInfo>,
}

/// Error during path resolution.
#[derive(Debug, Clone)]
pub enum PathResolutionError {
    /// Module not found in the registry
    ModuleNotFound(ModulePath),
    /// Item not found in the specified module
    ItemNotFound { module: ModulePath, item: String },
}
