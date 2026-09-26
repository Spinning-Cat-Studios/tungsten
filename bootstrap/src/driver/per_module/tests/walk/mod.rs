//! The module-tree walk's behaviour (ADR 14.8.26g): accumulation across the
//! walk, the order errors are assembled in, and the cache-commit gate.
//!
//! Grouped into a subdirectory by the 14.8.26g retrospective — three files on
//! one subject took `tests/` over the directory cap.

mod cache_commit_gate;
mod construction_site_poison;
mod error_assembly;
mod walk_accumulation;

// The on-disk project fixtures live in the parent `tests` module so the walk
// suite and the rest of per_module share one elaboration entry point.
use super::{
    elab_cache_entry_count, elaborate_tree_errors, elaborate_tree_errors_with_threads,
    elaborate_tree_with_cache, failing_files, make_parsed_module,
};
