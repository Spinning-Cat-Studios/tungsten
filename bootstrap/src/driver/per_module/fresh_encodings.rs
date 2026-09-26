//! Per-module fresh Phase-1e encodings — the normalization oracle's fresh side
//! (ADR 22.7.26b).
//!
//! For each module in the parsed tree, re-runs the collection pass (Phases
//! 1a–1e, no body elaboration) with the accumulated whole-project exports
//! injected — exactly the environment shape Body Elaboration used — and harvests the
//! Phase-1e encodings of the types *that module's own source declares*.
//! Injected export `TypeDef`s are overwritable (`defining_module: None`), so
//! `collect_type_def`/`collect_type_alias` re-elaborate each own-type body
//! from source AST and Encoding Finalization re-encodes it fresh: the resulting map is a
//! source-fresh re-derivation, independent of the stored encodings it is
//! compared against.
//!
//! This is deliberately off the hot path: it runs only under `doctor check
//! type normalization-consistency` (via the driver's inspector hook), one
//! module env at a time, collection-only. Unlike the whole-project
//! `normalize_for_comparison` pass (ADR 21.7.26j), the encodings it produces
//! fully expand records and generic instantiations — the 57%-skip class the
//! whole-project view could not faithfully check.

use std::collections::HashMap;

use tungsten_core::{Context, Type};

use crate::ast::Item;
use crate::elaborate::{collect_definitions_with_exports, ModuleExports};

use super::modules::ParsedModule;
use super::walk::body::build_module_mini_ast;
use super::BuildCtx;

/// Re-derive fresh Phase-1e encodings for every type declared in the module
/// tree, keyed by type name (matching the stored `encoded_types` key space).
///
/// A module whose collection pass fails is skipped — its types simply stay
/// absent from the map, and the consistency check reports them via its
/// fallback path rather than erroring the whole run.
pub(in crate::driver) fn per_module_fresh_encodings(
    module_tree: &ParsedModule,
    build: &BuildCtx<'_>,
    exports: &ModuleExports,
) -> HashMap<String, Type> {
    let mut modules: Vec<&ParsedModule> = Vec::new();
    collect_modules_post_order(module_tree, &mut modules);

    let mut fresh: HashMap<String, Type> = HashMap::new();
    for module in modules {
        harvest_module_own_encodings(module, build, exports, &mut fresh);
    }
    fresh
}

/// Collect every module in the tree post-order (children before parents),
/// matching Body Elaboration's elaboration order.
pub(super) fn collect_modules_post_order<'a>(
    module: &'a ParsedModule,
    out: &mut Vec<&'a ParsedModule>,
) {
    for child in &module.submodules {
        collect_modules_post_order(child, out);
    }
    out.push(module);
}

/// Type names a module's own source declares (`type` definitions + aliases).
/// Only these are harvested from its re-collection — every other env entry is
/// an injected export carrying a stored encoding, which would make the
/// comparison vacuous (stored vs stored).
pub(super) fn own_type_names(mini_ast: &crate::ast::SourceFile) -> Vec<String> {
    mini_ast
        .items
        .iter()
        .filter_map(|item| match item {
            Item::TypeDef(type_def) => Some(type_def.name.name.clone()),
            Item::TypeAlias(alias) => Some(alias.name.name.clone()),
            _ => None,
        })
        .collect()
}

/// Re-run collection for one module and merge its own types' fresh Phase-1e
/// encodings into `fresh`. First harvest wins on a cross-module name clash —
/// post-order matches Body Elaboration, and the stored map is name-flattened anyway.
fn harvest_module_own_encodings(
    module: &ParsedModule,
    build: &BuildCtx<'_>,
    exports: &ModuleExports,
    fresh: &mut HashMap<String, Type>,
) {
    let mini_ast = build_module_mini_ast(module);
    if mini_ast.items.is_empty() {
        return;
    }
    let own_types = own_type_names(&mini_ast);
    if own_types.is_empty() {
        return;
    }

    // Mirror Body Elaboration's per-module environment (body.rs `elaborate_module_fresh`):
    // per-module item→file mapping plus the accumulated exports.
    let mut module_info = build.module_info.clone();
    module_info.item_index_to_file = vec![module.path.clone(); mini_ast.items.len()];

    let mut local_ctx = Context::new();
    let Ok(collected) =
        collect_definitions_with_exports(&mini_ast, &mut local_ctx, module_info, exports)
    else {
        return;
    };
    // The pass defers errors instead of returning them (ADR 14.8.26g D2);
    // keep the pre-deferral semantics — a module whose collection failed
    // contributes no fresh encodings rather than partial ones.
    if collected.has_collection_errors() {
        return;
    }

    let module_encodings = collected.phase1e_encodings();
    for name in own_types {
        if let Some(encoding) = module_encodings.get(&name) {
            fresh.entry(name).or_insert_with(|| encoding.clone());
        }
    }
}
// Tests: tests/fresh_encodings.rs
