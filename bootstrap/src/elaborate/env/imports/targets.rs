//! Per-module import-target extraction (ADR 12.7.26a §2.1).
//!
//! Elaboration is the only phase that runs the real import machinery (plain,
//! alias, and glob imports, canonicalised through `pub use` chains). This
//! module exports that knowledge as a per-module table
//! `original name → canonical defining module path`, so codegen can resolve
//! colliding imported names without re-implementing `use` resolution.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::elaborate::env::Env;

/// Where a module's imported value canonically lives (ADR 12.7.26a §2.1).
///
/// `Ambiguous` encodes the double-alias case (`use a::f as fa; use b::f as
/// fb`): the original name maps to several defining modules, so codegen's D1
/// error names every candidate instead of picking a winner. Candidate lists
/// are sorted for deterministic cache bytes and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportTarget {
    /// The single canonical defining module path for the imported name.
    Unambiguous(Vec<String>),
    /// The same original name is imported from several modules (sorted).
    Ambiguous(Vec<Vec<String>>),
}

/// One module's import table: original (canonical) value name → its
/// canonical defining module. Keyed by original name because that is what
/// `Term::Global` carries after elaboration erases aliases.
pub type ValueImportTargets = BTreeMap<String, ImportTarget>;

impl Env {
    /// Extract this elaboration's value import targets (ADR 12.7.26a §2.1).
    ///
    /// Iterates the flat `imported_values` map — in per-module Body Elaboration that
    /// is exactly the current module's processed `use` items — resolves each
    /// through `pub use` chains to its canonical defining module, and keys
    /// the table by the canonical (original) name. Unresolvable entries are
    /// skipped: a colliding reference without a table entry hard-errors at
    /// codegen (D1) rather than resolving arbitrarily.
    pub fn extract_value_import_targets(&self) -> ValueImportTargets {
        let mut targets = ValueImportTargets::new();
        for info in self.imported_values.values() {
            let resolved =
                self.resolve_canonical_value_module(&info.original_name, &info.source_module);
            let Ok(Some((module, canonical_name))) = resolved else {
                continue;
            };
            let canonical_module = self.canonicalize_path(&module);
            record_import_target(&mut targets, canonical_name, canonical_module.segments);
        }
        targets
    }
}

/// Insert one resolved import into the table, upgrading to `Ambiguous` when
/// the same original name already maps to a different module.
fn record_import_target(targets: &mut ValueImportTargets, name: String, path: Vec<String>) {
    match targets.entry(name) {
        Entry::Vacant(vacant) => {
            vacant.insert(ImportTarget::Unambiguous(path));
        }
        Entry::Occupied(mut occupied) => merge_import_target(occupied.get_mut(), path),
    }
}

/// Merge a newly resolved defining module into an existing target for the
/// same original name: a second distinct module makes it `Ambiguous`.
fn merge_import_target(target: &mut ImportTarget, path: Vec<String>) {
    match target {
        ImportTarget::Unambiguous(existing) => {
            if *existing != path {
                let mut candidates = vec![existing.clone(), path];
                candidates.sort();
                *target = ImportTarget::Ambiguous(candidates);
            }
        }
        ImportTarget::Ambiguous(candidates) => {
            if !candidates.contains(&path) {
                candidates.push(path);
                candidates.sort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Extraction fixtures follow `env/resolution/visibility/tests/reexports.rs`:
    //! an `Env` built by hand from module registrations, value definitions,
    //! and `add_value_import` calls (the same registration path the real
    //! import machinery uses).

    use super::ImportTarget;
    use crate::ast::Visibility;
    use crate::elaborate::env::definitions::ValueDef;
    use crate::elaborate::env::{Env, ImportRequest, ModulePath};
    use crate::span::Span;

    fn define_value(env: &mut Env, module: &ModulePath, name: &str) {
        env.define_value_in_module(
            ValueDef {
                name: name.to_string(),
                ty: tungsten_core::Type::Nat,
                visibility: Visibility::Public,
                span: Span::new(0, 0),
            },
            module.clone(),
        );
    }

    fn import_value(
        env: &mut Env,
        importer: &ModulePath,
        local_name: &str,
        source: &ModulePath,
        original_name: &str,
    ) {
        env.add_value_import(
            importer,
            ImportRequest {
                local_name: local_name.to_string(),
                source_module: source.clone(),
                original_name: original_name.to_string(),
                span: Span::new(0, 0),
                is_reexport: false,
                reexport_visibility: None,
            },
        );
    }

    /// Env with modules `a`, `b` (each defining `describe`) and `main`.
    struct CollidingFixture {
        env: Env,
        module_a: ModulePath,
        module_b: ModulePath,
        main: ModulePath,
    }

    fn colliding_env() -> CollidingFixture {
        let mut env = Env::new();
        let module_a = ModulePath::from_name("a");
        let module_b = ModulePath::from_name("b");
        let main = ModulePath::from_name("main");
        env.register_module(module_a.clone());
        env.register_module(module_b.clone());
        env.register_module(main.clone());
        define_value(&mut env, &module_a, "describe");
        define_value(&mut env, &module_b, "describe");
        CollidingFixture {
            env,
            module_a,
            module_b,
            main,
        }
    }

    #[test]
    fn plain_import_maps_original_name_to_defining_module() {
        let CollidingFixture {
            mut env,
            module_a,
            main,
            ..
        } = colliding_env();
        import_value(&mut env, &main, "describe", &module_a, "describe");

        let targets = env.extract_value_import_targets();
        assert_eq!(
            targets.get("describe"),
            Some(&ImportTarget::Unambiguous(vec!["a".to_string()]))
        );
    }

    #[test]
    fn alias_import_keys_by_original_name() {
        let CollidingFixture {
            mut env,
            module_a,
            main,
            ..
        } = colliding_env();
        // `use a::{describe as d}` registers local `d`, original `describe`.
        import_value(&mut env, &main, "d", &module_a, "describe");

        let targets = env.extract_value_import_targets();
        assert!(
            !targets.contains_key("d"),
            "aliases must not appear as keys — Term::Global carries the original name"
        );
        assert_eq!(
            targets.get("describe"),
            Some(&ImportTarget::Unambiguous(vec!["a".to_string()]))
        );
    }

    #[test]
    fn reexport_chain_resolves_to_canonical_defining_module() {
        let CollidingFixture {
            mut env,
            module_a,
            main,
            ..
        } = colliding_env();
        // Module `c` re-exports `a::describe` the way the driver's `pub use`
        // pass records it: the name copied into `values` plus provenance.
        let module_c = ModulePath::from_name("c");
        env.register_module(module_c.clone());
        let contents = env.modules.get_mut(&module_c).unwrap();
        contents.values.push("describe".to_string());
        contents
            .reexported_value_sources
            .insert("describe".to_string(), (module_a, "describe".to_string()));

        // `use c::{describe}` in main.
        import_value(&mut env, &main, "describe", &module_c, "describe");

        let targets = env.extract_value_import_targets();
        assert_eq!(
            targets.get("describe"),
            Some(&ImportTarget::Unambiguous(vec!["a".to_string()])),
            "the chain must walk through `c` to the defining module"
        );
    }

    #[test]
    fn aliased_reexport_chain_recovers_original_name() {
        let CollidingFixture {
            mut env,
            module_a,
            main,
            ..
        } = colliding_env();
        // Module `c` has `pub use a::describe as pdesc`.
        let module_c = ModulePath::from_name("c");
        env.register_module(module_c.clone());
        let contents = env.modules.get_mut(&module_c).unwrap();
        contents.values.push("pdesc".to_string());
        contents
            .reexported_value_sources
            .insert("pdesc".to_string(), (module_a, "describe".to_string()));

        // `use c::{pdesc}` in main.
        import_value(&mut env, &main, "pdesc", &module_c, "pdesc");

        let targets = env.extract_value_import_targets();
        assert_eq!(
            targets.get("describe"),
            Some(&ImportTarget::Unambiguous(vec!["a".to_string()])),
            "the walk must rename `pdesc` back to the defining `describe`"
        );
    }

    #[test]
    fn double_alias_imports_marked_ambiguous() {
        let CollidingFixture {
            mut env,
            module_a,
            module_b,
            main,
        } = colliding_env();
        // `use a::{describe as da}; use b::{describe as db};`
        import_value(&mut env, &main, "da", &module_a, "describe");
        import_value(&mut env, &main, "db", &module_b, "describe");

        let targets = env.extract_value_import_targets();
        assert_eq!(
            targets.get("describe"),
            Some(&ImportTarget::Ambiguous(vec![
                vec!["a".to_string()],
                vec!["b".to_string()],
            ])),
            "same original name from two modules must be Ambiguous (sorted)"
        );
    }

    #[test]
    fn glob_style_registration_included() {
        // Glob imports register through the same `add_value_import` path with
        // local == original; the extraction is agnostic to the import form.
        let CollidingFixture {
            mut env,
            module_b,
            main,
            ..
        } = colliding_env();
        import_value(&mut env, &main, "describe", &module_b, "describe");

        let targets = env.extract_value_import_targets();
        assert_eq!(
            targets.get("describe"),
            Some(&ImportTarget::Unambiguous(vec!["b".to_string()]))
        );
    }

    #[test]
    fn unresolvable_import_skipped() {
        let CollidingFixture { mut env, main, .. } = colliding_env();
        let ghost = ModulePath::from_name("ghost");
        env.register_module(ghost.clone());
        import_value(&mut env, &main, "phantom", &ghost, "phantom");

        let targets = env.extract_value_import_targets();
        assert!(
            targets.is_empty(),
            "an import whose chain dead-ends must be skipped, not guessed: {targets:?}"
        );
    }
}
