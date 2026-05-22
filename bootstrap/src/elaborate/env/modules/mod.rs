//! Module system types and registry methods for Env.
//!
//! - `path`: Module path type and utilities
//! - `contents`: Module contents and path resolution errors
//! - `registry`: Module registry methods

mod contents;
mod path;
mod registry;

pub use contents::{ConstructorStubDetail, ModuleContents, PathResolutionError};
pub use path::ModulePath;
