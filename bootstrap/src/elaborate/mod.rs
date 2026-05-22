//! Elaborator: Surface AST to Core Terms
//!
//! The elaborator bridges the gap between the human-friendly surface syntax
//! and the mathematically-precise Core calculus that `tungsten_core` can process.
//!
//! ## Responsibilities
//!
//! 1. **Name resolution**: Resolve identifiers to definitions
//! 2. **Type inference**: Infer omitted type annotations (bidirectional)
//! 3. **Desugaring**: Convert surface constructs to core primitives
//! 4. **Validation**: Reject Phase 1 unsupported features with helpful errors
//! 5. **Core term construction**: Build `tungsten_core::Term` values
//!
//! ## Architecture
//!
//! ```text
//! Surface AST
//!      │
//!      ▼
//! ┌─────────────────────────────────────┐
//! │         NAME RESOLUTION             │
//! │  • Build symbol table from items    │
//! │  • Resolve identifiers              │
//! └──────────────────┬──────────────────┘
//!                    │
//!                    ▼
//! ┌─────────────────────────────────────┐
//! │    ELABORATION + TYPE INFERENCE     │
//! │  • Bidirectional type checking      │
//! │  • Infer omitted annotations        │

// Submodules
//! │  • Desugar surface constructs       │
//! │  • Build Core terms                 │
//! └──────────────────┬──────────────────┘
//!                    │
//!                    ▼
//!            Core Terms + Diagnostics
//! ```

mod codegen_types;
mod collection;
pub mod env;
mod error;
mod error_recording;
mod exprs;
mod items;
mod output;
pub(crate) mod phase_checks;
mod resolve_tyvars;
mod trace;
mod type_cache;
mod types;

#[cfg(test)]
mod tests;

pub use env::{
    Constructor, ConstructorInfo, ConstructorStubDetail, Env, ImportInfo, LocalBinding,
    ModuleContents, ModulePath, PathResolutionError, ResolvedValue, TypeDef, TypeDefKind, ValueDef,
};
pub use error::{ElabError, ElabErrorKind, ExpectedContext, ExpectedReason, Note, TraceFrame};
pub use output::elaborate_with_phase_checks;
pub use output::{
    AdtOrigin, CollectedElaborator, CollectionResult, CoreDef, ElabOutput, ElabResult,
    ModuleExports, TypeProvenance,
};
pub use phase_checks::{ElaborationPhase, PhaseCheckResult};

// Re-export top-level entry points
pub use output::{
    collect_definitions, collect_definitions_with_exports, collect_definitions_with_modules,
    elaborate, elaborate_with_warnings, elaborate_with_warnings_full,
};

use tungsten_core::Context;

/// The elaborator state machine.
///
/// Elaboration mode — controls expect_type behaviour (ADR 4.5.26g §2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElabMode {
    /// tungsten check: validate expect_type args, skip comparison
    #[default]
    Check,
    /// tungsten test: validate + compare types
    Test,
    /// tungsten compile/run: reject expect_type
    Compile,
}

/// Holds all context needed during elaboration:
/// - Name resolution environment
/// - Reference to Core context (for validation)
/// - Accumulated errors
/// - Current de Bruijn depth
/// - Context stack for "expected because..." messages
pub struct Elaborator<'a> {
    /// Name resolution environment
    env: Env,
    /// Reference to the Core context (for type validation)
    #[allow(dead_code)]
    core_ctx: &'a mut Context,
    /// Accumulated errors (we try to continue after errors)
    errors: Vec<ElabError>,
    /// Accumulated warnings (non-fatal diagnostics)
    warnings: Vec<ElabError>,
    /// Current de Bruijn depth for local variables
    depth: usize,
    /// Stack of type expectation contexts for error messages
    context_stack: Vec<ExpectedContext>,
    /// Counter for generating unique names
    name_counter: usize,
    /// Current module path (for visibility checking)
    current_module: ModulePath,
    /// Name of the definition currently being elaborated (for --trace-types)
    current_def_name: Option<String>,
    /// Target definition for type tracing (--trace-types=<name>)
    trace_target: Option<String>,
    /// Target type for encoding tracing (--trace-encoding=<name>)
    trace_encoding: Option<String>,
    /// Target type for normalization tracing (--trace-normalization=<name>)
    trace_normalization: Option<String>,
    /// Trace constructor registration calls (--trace-constructor-registration, ADR 7.5.26e)
    trace_ctor_registration: bool,
    /// Type provenance map built during ADT encoding (ADR 13.4.26c §3)
    type_provenance: TypeProvenance,
    /// Mutual recursion group membership (ADR 18.4.26i §5).
    /// Maps each type name → the full group (all members including itself).
    /// Only populated for types in SCCs of size > 1.
    mutual_recursion_groups: std::collections::HashMap<String, Vec<String>>,
    /// True during Phase 1c (type collection). ADT cross-references are deferred
    /// as TyVar("@Name") instead of eagerly encoded, so mutual recursion groups
    /// can be computed before encoding (ADR 18.4.26i §5).
    pub(crate) collection_phase: bool,
    /// When true, run phase invariant checks after each pipeline phase (ADR 20.4.26e).
    pub(crate) check_phase_invariants: bool,
    /// Accumulated phase invariant check results (ADR 20.4.26e).
    pub(crate) phase_invariant_results: Vec<phase_checks::PhaseCheckResult>,
    /// Debug-mode consistency map: records the first recursiveness decision for each
    /// ADT name and panics if a subsequent call returns a different result.
    /// Zero overhead in release builds. (ADR 21.4.26c)
    #[cfg(debug_assertions)]
    pub(crate) recursiveness_decisions: std::cell::RefCell<std::collections::HashMap<String, bool>>,
    /// Elaboration mode (ADR 4.5.26g)
    pub(crate) elab_mode: ElabMode,
    /// When true, value collection (functions, extern fns, theorems, axioms)
    /// overwrites existing entries rather than reporting duplicates. Set by
    /// Phase A.5's `collect_definitions_with_exports()` to allow re-collection
    /// of values that were pre-registered by Phase A stubs (ADR 5.5.26c).
    pub(crate) allow_value_overwrite: bool,
    /// The declared return type of the current function (for `return` expressions).
    /// Set when entering a function body, reset on exit.
    pub(crate) current_return_type: Option<tungsten_core::Type>,
}

impl<'a> Elaborator<'a> {
    /// Create a new elaborator with an empty environment.
    pub fn new(core_ctx: &'a mut Context) -> Self {
        Self {
            env: Env::new(),
            core_ctx,
            errors: Vec::new(),
            warnings: Vec::new(),
            depth: 0,
            context_stack: Vec::new(),
            name_counter: 0,
            current_module: ModulePath::root(),
            current_def_name: None,
            trace_target: None,
            trace_encoding: None,
            trace_normalization: None,
            trace_ctor_registration: false,
            type_provenance: TypeProvenance::default(),
            mutual_recursion_groups: std::collections::HashMap::new(),
            collection_phase: false,
            check_phase_invariants: false,
            phase_invariant_results: Vec::new(),
            #[cfg(debug_assertions)]
            recursiveness_decisions: std::cell::RefCell::new(std::collections::HashMap::new()),
            elab_mode: ElabMode::Check,
            allow_value_overwrite: false,
            current_return_type: None,
        }
    }
    /// Execute `f` with `current_return_type` set to `ty`, restoring the previous
    /// value afterward (even on error). Use this instead of manual save/restore.
    pub(crate) fn with_return_context<T>(
        &mut self,
        ty: Option<tungsten_core::Type>,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let prev = std::mem::replace(&mut self.current_return_type, ty);
        let result = f(self);
        self.current_return_type = prev;
        result
    }

    /// Generate a fresh unique variable name with the given prefix.
    pub fn fresh_var(&mut self, prefix: &str) -> String {
        let name = format!("__{}{}", prefix, self.name_counter);
        self.name_counter += 1;
        name
    }

    /// Get the current module path.
    pub fn get_current_module(&self) -> &ModulePath {
        &self.current_module
    }

    /// Set the current module path (for entering nested modules).
    pub fn set_current_module(&mut self, module: ModulePath) {
        self.current_module = module;
    }

    /// Enable phase invariant checking (ADR 20.4.26e).
    pub fn set_check_phase_invariants(&mut self, enabled: bool) {
        self.check_phase_invariants = enabled;
    }

    /// Take the accumulated phase invariant results, leaving the vec empty.
    pub fn take_phase_invariant_results(&mut self) -> Vec<phase_checks::PhaseCheckResult> {
        std::mem::take(&mut self.phase_invariant_results)
    }
}
