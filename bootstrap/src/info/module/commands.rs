//! CLI definition for `tungsten info module <subcommand>`.
//!
//! Extracted from `info/mod.rs` (file-size convention); the handlers live in
//! sibling modules and dispatch via `super::dispatch_module_info`.

use std::path::PathBuf;

use clap::Subcommand;

/// Module-related info subcommands (ADR 6.5.26a, 8.5.26f).
///
/// Grouped to keep the `info` namespace manageable. Accessed via
/// `tungsten info module <subcommand>`.
#[derive(Subcommand)]
pub enum ModuleInfoCommands {
    /// Visualize the module hierarchy, elaboration order, and cross-branch deps (ADR 6.5.26a)
    ///
    /// Shows the containment tree, dependency-sorted elaboration sequence,
    /// and cross-branch import edges. Cost ≤ 2 (parse only).
    ///
    /// Examples:
    ///   tungsten info module tree `examples/module_example/main.tg`
    ///   tungsten info module tree src/compiler/main.tg
    Tree {
        /// The root source file of the project
        file: PathBuf,
    },

    /// Show import resolution status for a module (ADR 6.5.26a)
    ///
    /// Lists each `use` declaration and whether imported names resolved
    /// to full definitions or stubs after elaboration.
    ///
    /// Examples:
    ///   tungsten info module imports `driver::ffi` src/compiler/main.tg
    ///   tungsten info module imports parser `examples/module_example/main.tg`
    Imports {
        /// Fully qualified module path (e.g., "`driver::ffi`")
        module: String,

        /// The root source file of the project
        file: PathBuf,
    },

    /// Trace re-export chain for a module's items (ADR 8.5.26f)
    ///
    /// Shows how items from a module propagate through `pub use`
    /// declarations in the module tree.
    ///
    /// Examples:
    ///   tungsten info module reexport-chain child `examples/module_example/main.tg`
    ///   tungsten info module reexport-chain `elab::env` src/compiler/main.tg
    ReexportChain {
        /// Fully qualified module path (e.g., "child", "`elab::env`")
        module: String,

        /// The root source file of the project
        file: PathBuf,
    },

    /// Show import alias mappings for a module (ADR 16.5.26b)
    ///
    /// Lists each aliased import (`use X as Y`) showing the local alias name,
    /// the original name, and the source path. Aliased names suppress the
    /// original in that module's scope. Cost ≤ 2 (parse only).
    ///
    /// Examples:
    ///   tungsten info module alias-table math `tests/import_alias/main.tg`
    ///   tungsten info module alias-table `driver::ffi` src/compiler/main.tg
    #[command(name = "alias-table")]
    AliasTable {
        /// Fully qualified module path (e.g., "math", "`driver::ffi`")
        module: String,

        /// The root source file of the project
        file: PathBuf,
    },

    /// Show the value-import-target table codegen uses to resolve colliding
    /// names (ADR 12.7.26a)
    ///
    /// For a module, lists each imported value name and the canonical defining
    /// module codegen binds it to (`→` unambiguous, `⚠` ambiguous). Ambiguous
    /// names hard-error at codegen (D1), so this is a pre-codegen ambiguity
    /// probe. Cost ≤ 3 (elaborate). See also: `info module imports` (raw
    /// resolution status), `doctor check extern-map-ambiguity` (codegen gate).
    ///
    /// Examples:
    ///   tungsten info module import-targets main `tests/collide/main.tg`
    ///   tungsten info module import-targets `elab::env` src/compiler/main.tg
    #[command(name = "import-targets")]
    ImportTargets {
        /// Fully qualified module path (e.g., "main", "`elab::env`")
        module: String,

        /// The root source file of the project
        file: PathBuf,
    },

    /// Show who depends on a module, and by which path (ADR 5.9.26f)
    ///
    /// The inverse of `info module imports`: the question every directory
    /// regroup asks. Reports two populations separately, because the
    /// difference is the whole answer — a **direct** dependent names the
    /// module in its path (`driver::ffi::types::…`) and breaks when the
    /// module moves; an **indirect** one reaches the same items through a
    /// re-export (`driver::ffi::{…}`) and does not. `grep` cannot tell them
    /// apart. A third section lists `.tg` string literals containing the
    /// module path — real dependencies no resolved table can see, found by
    /// text and labelled as such. `--verbose` lists the indirect sites
    /// instead of counting them per re-export hop. Cost ≤ 3 (elaborate):
    /// it refuses a corpus that does not elaborate rather than answering
    /// from an untrustworthy resolution.
    ///
    /// Examples:
    ///   tungsten info module dependents `driver::ffi::types` src/compiler/main.tg
    ///   tungsten info module dependents child `examples/module_example/main.tg`
    Dependents {
        /// Fully qualified module path (e.g., "`driver::ffi::types`")
        module: String,

        /// The root source file of the project
        file: PathBuf,
    },
}
