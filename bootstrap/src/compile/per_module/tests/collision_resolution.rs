//! LLVM-free unit tests for colliding-reference resolution (ADR 12.7.26a).
//!
//! `collision_overrides_for_unit` is a pure function over maps; these tests
//! pin the resolution order (own-module → import-table → hard error), the
//! `foo/mod.tg` D4 regression, and the §2.3 diagnostic shape.

use std::collections::{BTreeMap, BTreeSet};

use tungsten_bootstrap::driver::ValueImportTargetsByModule;
use tungsten_bootstrap::elaborate::{ImportTarget, ValueImportTargets};
use tungsten_core::types::Type;

use crate::compile::per_module::compilation::DefInfo;
use crate::compile::per_module::imports::{collision_overrides_for_unit, CollisionContext};

/// A `DefInfo` whose unit/module naming mirrors per-function units:
/// unit `<module>__<name>` (or an explicit unit for `mod.tg` cases).
fn colliding_def(owner_unit: &str, module_path: &[&str], llvm_name: &str) -> DefInfo {
    DefInfo {
        llvm_name: llvm_name.to_string(),
        ty: Type::Nat,
        owner_unit: owner_unit.to_string(),
        module_path: module_path.iter().map(|s| s.to_string()).collect(),
    }
}

/// The standard two-way collision: `a::describe` and `b::describe`.
struct CollisionFixture {
    all_defs_info: BTreeMap<String, DefInfo>,
    collision_index: BTreeMap<String, Vec<String>>,
    import_targets: ValueImportTargetsByModule,
}

impl CollisionFixture {
    fn new() -> Self {
        let mut all_defs_info = BTreeMap::new();
        all_defs_info.insert(
            "a__describe::describe".to_string(),
            colliding_def("a__describe", &["a"], "a__describe__describe"),
        );
        all_defs_info.insert(
            "b__describe::describe".to_string(),
            colliding_def("b__describe", &["b"], "b__describe__describe"),
        );
        let mut collision_index = BTreeMap::new();
        collision_index.insert(
            "describe".to_string(),
            vec![
                "a__describe::describe".to_string(),
                "b__describe::describe".to_string(),
            ],
        );
        Self {
            all_defs_info,
            collision_index,
            import_targets: ValueImportTargetsByModule::new(),
        }
    }

    fn with_import(mut self, module: &[&str], name: &str, target: ImportTarget) -> Self {
        let mut table = ValueImportTargets::new();
        table.insert(name.to_string(), target);
        self.import_targets
            .insert(module.iter().map(|s| s.to_string()).collect(), table);
        self
    }

    fn ctx(&self) -> CollisionContext<'_> {
        CollisionContext {
            collision_index: &self.collision_index,
            all_defs_info: &self.all_defs_info,
            import_targets: &self.import_targets,
        }
    }
}

fn referenced(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn own_module_def_wins_over_import_table() {
    // Unit in module `a` referencing `describe`: even with an import table
    // entry pointing at `b`, the own-module sibling wins (resolution order).
    let fixture = CollisionFixture::new().with_import(
        &["a"],
        "describe",
        ImportTarget::Unambiguous(vec!["b".to_string()]),
    );
    let overrides = collision_overrides_for_unit(
        &["a".to_string()],
        "a__caller",
        &referenced(&["describe"]),
        &fixture.ctx(),
    )
    .unwrap();
    assert_eq!(
        overrides.get("describe").map(String::as_str),
        Some("a__describe__describe")
    );
}

#[test]
fn import_target_selects_the_imported_module() {
    // Unit in module `main` (defines nothing) importing from `a`.
    let fixture = CollisionFixture::new().with_import(
        &["main"],
        "describe",
        ImportTarget::Unambiguous(vec!["a".to_string()]),
    );
    let overrides = collision_overrides_for_unit(
        &["main".to_string()],
        "main__tungsten_main",
        &referenced(&["describe"]),
        &fixture.ctx(),
    )
    .unwrap();
    assert_eq!(
        overrides.get("describe").map(String::as_str),
        Some("a__describe__describe")
    );
}

#[test]
fn ambiguous_import_is_a_hard_error_with_the_233_diagnostic() {
    let fixture = CollisionFixture::new().with_import(
        &["main"],
        "describe",
        ImportTarget::Ambiguous(vec![vec!["a".to_string()], vec!["b".to_string()]]),
    );
    let err = collision_overrides_for_unit(
        &["main".to_string()],
        "main__tungsten_main",
        &referenced(&["describe"]),
        &fixture.ctx(),
    )
    .unwrap_err();
    // The §2.3 shape: unit-scoped, names both modules, suggests the fix.
    assert!(
        err.contains("ambiguous reference to `describe` in unit `main__tungsten_main`"),
        "{err}"
    );
    assert!(err.contains("defined in modules `a` and `b`"), "{err}");
    assert!(err.contains("help: add `use a::{describe}`"), "{err}");
    assert!(err.contains("use ... as ..."), "{err}");
}

#[test]
fn unimported_colliding_reference_is_a_hard_error() {
    let fixture = CollisionFixture::new();
    let err = collision_overrides_for_unit(
        &["main".to_string()],
        "main__tungsten_main",
        &referenced(&["describe"]),
        &fixture.ctx(),
    )
    .unwrap_err();
    assert!(err.contains("ambiguous reference to `describe`"), "{err}");
}

#[test]
fn non_colliding_names_pass_through_untouched() {
    let fixture = CollisionFixture::new();
    let overrides = collision_overrides_for_unit(
        &["main".to_string()],
        "main__tungsten_main",
        &referenced(&["helper", "unrelated"]),
        &fixture.ctx(),
    )
    .unwrap();
    assert!(
        overrides.is_empty(),
        "names outside the collision set must not be overridden: {overrides:?}"
    );
}

#[test]
fn import_target_naming_a_module_without_the_def_errors() {
    // Defensive: the table says `ghost`, but no candidate lives there.
    let fixture = CollisionFixture::new().with_import(
        &["main"],
        "describe",
        ImportTarget::Unambiguous(vec!["ghost".to_string()]),
    );
    let err = collision_overrides_for_unit(
        &["main".to_string()],
        "main__tungsten_main",
        &referenced(&["describe"]),
        &fixture.ctx(),
    )
    .unwrap_err();
    assert!(err.contains("ambiguous reference"), "{err}");
}

/// The D4 regression: a module defined in `foo/mod.tg` has unit base
/// `foo__mod` while its module path is `["foo"]`. The old depot override
/// reconstructed the owner key as `foo__pick::pick` (from
/// `module_path.join("__")`) and silently missed; matching on
/// `DefInfo.module_path` resolves it.
#[test]
fn mod_tg_unit_name_divergence_resolves_via_module_path() {
    let mut all_defs_info = BTreeMap::new();
    // Unit base is file-path-derived: foo/mod.tg → `foo__mod__pick`.
    all_defs_info.insert(
        "foo__mod__pick::pick".to_string(),
        colliding_def("foo__mod__pick", &["foo"], "foo__mod__pick__pick"),
    );
    all_defs_info.insert(
        "other__pick::pick".to_string(),
        colliding_def("other__pick", &["other"], "other__pick__pick"),
    );
    let mut collision_index = BTreeMap::new();
    collision_index.insert(
        "pick".to_string(),
        vec![
            "foo__mod__pick::pick".to_string(),
            "other__pick::pick".to_string(),
        ],
    );
    let import_targets = ValueImportTargetsByModule::new();
    let ctx = CollisionContext {
        collision_index: &collision_index,
        all_defs_info: &all_defs_info,
        import_targets: &import_targets,
    };

    // A depot instance whose generic def lives in module `foo` references
    // its own-module `pick`.
    let overrides = collision_overrides_for_unit(
        &["foo".to_string()],
        "__mono[inst]",
        &referenced(&["pick"]),
        &ctx,
    )
    .unwrap();
    assert_eq!(
        overrides.get("pick").map(String::as_str),
        Some("foo__mod__pick__pick"),
        "module-path matching must find the foo/mod.tg def the old \
         `module_path.join(\"__\")` key reconstruction missed"
    );
}
