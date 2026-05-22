//! Pub use re-export processing for module info.
//!
//! Handles `pub use` declarations by copying items from source modules
//! to the re-exporting module's contents.

use crate::ast::{ExpandedUseTree, Item, Visibility};
use crate::elaborate::{ModuleContents, ModulePath};

use super::info::{get_module_name, ModuleInfo};
use super::ParsedModule;

/// Process `pub use` re-exports to make re-exported items visible in the module.
///
/// This is a second pass after all modules and items are registered, so we can
/// resolve the source modules and copy their items to the re-exporting module.
///
/// Uses post-order traversal (children before parent) so that a parent's
/// `pub use child::*` sees the child's own re-exports already applied.
/// Without post-order, chained re-exports through nested directory modules
/// produce incomplete copies (ADR 8.5.26a).
pub(super) fn process_pub_use_reexports(
    module: &ParsedModule,
    current_path: &ModulePath,
    info: &mut ModuleInfo,
) {
    // Recursively process submodules FIRST (post-order)
    for submodule in &module.submodules {
        let module_name = get_module_name(submodule);
        let child_path = current_path.child(module_name);
        process_pub_use_reexports(submodule, &child_path, info);
    }

    // Then process pub use declarations in this module
    for item in &module.source_file.items {
        if let Item::Use(use_decl) = item {
            // Only process `pub use` declarations
            if !matches!(use_decl.visibility, Visibility::Public | Visibility::Crate) {
                continue;
            }

            // Expand the use tree
            match use_decl.tree.expand() {
                ExpandedUseTree::Paths(paths) => {
                    for path in paths {
                        // Get module path (all but last segment) and item name (last segment)
                        if path.segments.len() >= 2 {
                            let item_name = path.segments.last().unwrap().name.clone();
                            let module_segments: Vec<String> = path.segments
                                [..path.segments.len() - 1]
                                .iter()
                                .map(|s| s.name.clone())
                                .collect();

                            // Try to resolve the source module
                            let source_module =
                                resolve_pub_use_module(&module_segments, current_path, info);

                            if let Some(src_mod) = source_module {
                                // Copy item from source module to current module
                                copy_item_to_module(&src_mod, &item_name, current_path, info);
                            }
                        }
                    }
                }
                ExpandedUseTree::Glob { prefix, .. } => {
                    // pub use foo::* - copy all items from foo
                    let module_segments: Vec<String> =
                        prefix.segments.iter().map(|s| s.name.clone()).collect();

                    let source_module =
                        resolve_pub_use_module(&module_segments, current_path, info);

                    if let Some(src_mod) = source_module {
                        // Copy all items from source module
                        copy_all_items_to_module(&src_mod, current_path, info);
                    }
                }
                ExpandedUseTree::Alias { path, alias, .. } => {
                    // pub use foo::Bar as Baz - copy Bar from foo, register as Baz
                    if path.segments.len() >= 2 {
                        let item_name = path.segments.last().unwrap().name.clone();
                        let module_segments: Vec<String> = path.segments[..path.segments.len() - 1]
                            .iter()
                            .map(|s| s.name.clone())
                            .collect();

                        let source_module =
                            resolve_pub_use_module(&module_segments, current_path, info);

                        if let Some(src_mod) = source_module {
                            copy_item_to_module_as(
                                &src_mod,
                                &item_name,
                                &alias.name,
                                current_path,
                                info,
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Resolve a module path for pub use (relative to current module).
pub fn resolve_pub_use_module(
    segments: &[String],
    current_path: &ModulePath,
    info: &ModuleInfo,
) -> Option<ModulePath> {
    if segments.is_empty() {
        return None;
    }

    // Build the raw path
    let raw_path = ModulePath::from_segments(segments);

    // Try child resolution first (e.g., "common" in ast → ast::common)
    let child_path = current_path.join(&raw_path);
    if info.modules.contains_key(&child_path) {
        return Some(child_path);
    }

    // Try sibling resolution
    if let Some(parent) = current_path.parent() {
        let sibling_path = parent.join(&raw_path);
        if info.modules.contains_key(&sibling_path) {
            return Some(sibling_path);
        }
    }

    // Try absolute resolution
    if info.modules.contains_key(&raw_path) {
        return Some(raw_path);
    }

    None
}

/// Copy entries from source module contents to target module contents.
///
/// When `filter` is `Some(name)`, only the item matching that name is copied.
/// When `filter` is `None`, all items are copied (glob re-export).
///
/// Uses insert-if-absent semantics: existing items in the target are never
/// overwritten. Visibility and type param counts are propagated from source.
fn copy_contents_entries(src: &ModuleContents, tgt: &mut ModuleContents, filter: Option<&str>) {
    // Copy types with visibility and type param counts
    for name in &src.types {
        if filter.is_some_and(|f| f != name) {
            continue;
        }
        if !tgt.types.iter().any(|n| n == name) {
            tgt.types.push(name.clone());
            if let Some(vis) = src.type_visibility.get(name) {
                tgt.type_visibility.insert(name.clone(), vis.clone());
            }
            // Copy type param count for generic types (ADR 30.1.26.1)
            if let Some(&count) = src.type_param_counts.get(name) {
                tgt.type_param_counts.insert(name.clone(), count);
            }
        }
    }

    // Copy values with visibility
    for name in &src.values {
        if filter.is_some_and(|f| f != name) {
            continue;
        }
        if !tgt.values.iter().any(|n| n == name) {
            tgt.values.push(name.clone());
            if let Some(vis) = src.value_visibility.get(name) {
                tgt.value_visibility.insert(name.clone(), vis.clone());
            }
        }
    }

    // Copy constructors with visibility
    for name in &src.constructors {
        if filter.is_some_and(|f| f != name) {
            continue;
        }
        if !tgt.constructors.iter().any(|n| n == name) {
            tgt.constructors.push(name.clone());
            if let Some(vis) = src.constructor_visibility.get(name) {
                tgt.constructor_visibility.insert(name.clone(), vis.clone());
            }
        }
    }
}

/// Copy a specific item from source module to target module.
fn copy_item_to_module(
    source_module: &ModulePath,
    item_name: &str,
    target_module: &ModulePath,
    info: &mut ModuleInfo,
) {
    let source_contents = match info.modules.get(source_module) {
        Some(c) => c.clone(),
        None => return,
    };
    let target_contents = info.modules.entry(target_module.clone()).or_default();
    copy_contents_entries(&source_contents, target_contents, Some(item_name));
}

/// Copy all items from source module to target module (for glob re-exports).
fn copy_all_items_to_module(
    source_module: &ModulePath,
    target_module: &ModulePath,
    info: &mut ModuleInfo,
) {
    let source_contents = match info.modules.get(source_module) {
        Some(c) => c.clone(),
        None => return,
    };
    let target_contents = info.modules.entry(target_module.clone()).or_default();
    copy_contents_entries(&source_contents, target_contents, None);
}

/// Copy a specific item from source module to target module under an alias name.
fn copy_item_to_module_as(
    source_module: &ModulePath,
    item_name: &str,
    alias_name: &str,
    target_module: &ModulePath,
    info: &mut ModuleInfo,
) {
    let source_contents = match info.modules.get(source_module) {
        Some(c) => c.clone(),
        None => return,
    };
    let target_contents = info.modules.entry(target_module.clone()).or_default();

    // Copy type if it exists under item_name, register as alias_name
    if source_contents.types.iter().any(|n| n == item_name) {
        if !target_contents.types.iter().any(|n| n == alias_name) {
            target_contents.types.push(alias_name.to_string());
            if let Some(vis) = source_contents.type_visibility.get(item_name) {
                target_contents
                    .type_visibility
                    .insert(alias_name.to_string(), vis.clone());
            }
            if let Some(&count) = source_contents.type_param_counts.get(item_name) {
                target_contents
                    .type_param_counts
                    .insert(alias_name.to_string(), count);
            }
        }
    }

    // Copy value if it exists under item_name, register as alias_name
    if source_contents.values.iter().any(|n| n == item_name) {
        if !target_contents.values.iter().any(|n| n == alias_name) {
            target_contents.values.push(alias_name.to_string());
            if let Some(vis) = source_contents.value_visibility.get(item_name) {
                target_contents
                    .value_visibility
                    .insert(alias_name.to_string(), vis.clone());
            }
        }
    }

    // Copy constructor if it exists under item_name, register as alias_name
    if source_contents.constructors.iter().any(|n| n == item_name) {
        if !target_contents.constructors.iter().any(|n| n == alias_name) {
            target_contents.constructors.push(alias_name.to_string());
            if let Some(vis) = source_contents.constructor_visibility.get(item_name) {
                target_contents
                    .constructor_visibility
                    .insert(alias_name.to_string(), vis.clone());
            }
        }
    }
}
