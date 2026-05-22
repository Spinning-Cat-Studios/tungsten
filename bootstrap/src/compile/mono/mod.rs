//! Single-owner monomorphization infrastructure (ADR 6.5.26f).
//!
//! Assigns each monomorphized instance `(DefId, CanonicalTypeArgs)` to exactly
//! one codegen unit, eliminating duplicate work across units and enabling
//! future parallel codegen (W8).
//!
//! # Pipeline
//!
//! ```text
//! Elaboration → Discovery → Freeze → Ownership → Symbol gen → Codegen
//!                (mutable)   (lock)   (immutable)  (immutable)  (read-only)
//! ```

mod discovery;
mod map;
mod ownership;
mod symbols;
mod table;

#[cfg(test)]
mod tests;

pub use discovery::discover_mono_requests;
pub use map::MonoOwnershipMap;
pub use ownership::assign_owners;
#[allow(unused_imports)]
// Stage 5 (ADR 8.5.26g): used once mono pipeline is wired into codegen
pub use symbols::{mangle_mono_symbol, validate_symbols};
pub use table::MonoRequestTable;

use std::fmt;

use serde::{Deserialize, Serialize};

use tungsten_core::types::Type;

// ── DefId ───────────────────────────────────────────────────────────

/// Stable identity for a top-level definition: owning module path + name.
///
/// Unlike bare strings, `DefId` is unambiguous across modules — two
/// definitions with the same name in different modules have distinct
/// `DefId`s. This is the canonical identifier used for mono keys.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct DefId {
    /// Module path segments (e.g., `["compiler", "lexer"]`)
    pub module_path: Vec<String>,
    /// Definition name (e.g., `"list_reverse"`)
    pub name: String,
}

impl DefId {
    pub fn new(module_path: Vec<String>, name: impl Into<String>) -> Self {
        Self {
            module_path,
            name: name.into(),
        }
    }

    /// The codegen unit that owns this definition (derived from module path).
    ///
    /// Not used for ownership assignment (all mono goes to `__mono` depot),
    /// but retained for diagnostics and test coverage.
    #[allow(dead_code)]
    pub fn owner_unit_id(&self) -> CodegenUnitId {
        CodegenUnitId(self.module_path.join("__"))
    }
}

impl fmt::Display for DefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.module_path.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}::{}", self.module_path.join("::"), self.name)
        }
    }
}

// ── CanonicalTypeArgs ───────────────────────────────────────────────

/// Normalized string representation of type arguments for a mono instance.
///
/// Two alpha-equivalent recursive types must produce the same canonical
/// string. Non-equivalent types must produce different strings.
/// Uses `normalize_for_comparison` from the elaboration pipeline.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct CanonicalTypeArgs(pub String);

impl CanonicalTypeArgs {
    /// Create canonical type args from a Core type.
    ///
    /// Uses `Debug` format of the normalized type as the canonical string.
    /// This is deterministic and captures the full type structure.
    pub fn from_type(ty: &Type) -> Self {
        Self(format!("{:?}", ty))
    }

    /// Create canonical type args from multiple Core types.
    ///
    /// For a single type arg, this is equivalent to `from_type`.
    /// For multiple type args, joins their Debug representations with `, `.
    pub fn from_types(tys: &[Type]) -> Self {
        if tys.len() == 1 {
            Self::from_type(&tys[0])
        } else {
            let parts: Vec<String> = tys.iter().map(|t| format!("{:?}", t)).collect();
            Self(parts.join(", "))
        }
    }
}

impl fmt::Display for CanonicalTypeArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── MonoKey ─────────────────────────────────────────────────────────

/// Canonical key identifying a unique monomorphized instance.
///
/// Uses `DefId` (not user-facing name) to avoid aliasing/shadow issues.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct MonoKey {
    pub def_id: DefId,
    pub type_args: CanonicalTypeArgs,
}

impl MonoKey {
    pub fn new(def_id: DefId, type_args: CanonicalTypeArgs) -> Self {
        Self { def_id, type_args }
    }
}

impl fmt::Display for MonoKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}<{}>", self.def_id, self.type_args)
    }
}

// ── CodegenUnitId ───────────────────────────────────────────────────

/// Identifier for a codegen unit (derived from source file path).
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct CodegenUnitId(pub String);

/// Synthetic codegen unit for all monomorphized specializations (ADR 9.5.26b §2.3).
pub const MONO_DEPOT_UNIT: &str = "__mono";

impl CodegenUnitId {
    pub fn mono_depot() -> Self {
        Self(MONO_DEPOT_UNIT.to_string())
    }

    #[allow(dead_code)] // used in tests; useful API for future pipeline guards
    pub fn is_mono_depot(&self) -> bool {
        self.0 == MONO_DEPOT_UNIT
    }
}

impl fmt::Display for CodegenUnitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── MonoRequest ─────────────────────────────────────────────────────

/// A request from a codegen unit for a monomorphized instance.
#[derive(Debug, Clone)]
pub struct MonoRequest {
    pub key: MonoKey,
    pub requester_unit: CodegenUnitId,
    /// The original type arguments (needed for codegen compilation).
    /// Single-type-param generics have one element; multi-type-param have multiple.
    pub type_args: Vec<Type>,
}

// ── MonoOwnership ───────────────────────────────────────────────────

/// Immutable assignment of a monomorphized instance to its owner.
#[derive(Debug, Clone)]
pub struct MonoOwnership {
    pub key: MonoKey,
    pub owner_unit: CodegenUnitId,
    pub symbol: String,
    /// The original type arguments (needed for codegen compilation).
    /// Single-type-param generics have one element; multi-type-param have multiple.
    pub type_args: Vec<Type>,
}
