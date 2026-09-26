//! Shared fixtures for `per_module` unit tests.
//!
//! `UnitCompileCtx` borrows eleven project-level values; [`TestCtxParts`]
//! owns minimal defaults for all of them so a test can construct a context
//! from just the `DefInfo` map it cares about. Extend here (not per test
//! file) when `UnitCompileCtx` grows a field — one fixture to update.

use crate::compile::per_module::*;
use compilation::DefInfo;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use tungsten_core::types::Type;

use crate::compile::mono::{MonoOwnershipMap, MonoRequestTable};
use crate::compile::CompileFlags;

/// Owned backing storage for a minimal `UnitCompileCtx`.
pub(in crate::compile::per_module) struct TestCtxParts {
    all_defs_info: BTreeMap<String, DefInfo>,
    collisions: HashSet<String>,
    collision_index: BTreeMap<String, Vec<String>>,
    import_targets: tungsten_bootstrap::driver::ValueImportTargetsByModule,
    comparator_types: tungsten_bootstrap::comparator::ComparatorTypes,
    adt_types: HashMap<String, (Vec<String>, Vec<tungsten_codegen::CodegenConstructor>)>,
    main_ty: Type,
    file: PathBuf,
    source_root: PathBuf,
    mono_map: MonoOwnershipMap,
    mono_table: MonoRequestTable,
    poly_term_registry: HashMap<String, tungsten_core::terms::Term>,
    flags: CompileFlags,
}

impl TestCtxParts {
    pub(in crate::compile::per_module) fn with_defs(
        all_defs_info: BTreeMap<String, DefInfo>,
    ) -> Self {
        Self {
            all_defs_info,
            collisions: HashSet::new(),
            collision_index: BTreeMap::new(),
            import_targets: tungsten_bootstrap::driver::ValueImportTargetsByModule::new(),
            comparator_types: tungsten_bootstrap::comparator::ComparatorTypes::default(),
            adt_types: HashMap::new(),
            main_ty: Type::Nat,
            file: PathBuf::from("test.tg"),
            source_root: PathBuf::new(),
            mono_map: MonoOwnershipMap::new(HashMap::new()),
            mono_table: MonoRequestTable::new(),
            poly_term_registry: HashMap::new(),
            flags: CompileFlags::default(),
        }
    }

    pub(in crate::compile::per_module) fn ctx(&self) -> UnitCompileCtx<'_> {
        UnitCompileCtx {
            all_defs_info: &self.all_defs_info,
            collisions: &self.collisions,
            collision_index: &self.collision_index,
            import_targets: &self.import_targets,
            project: ProjectCtx {
                comparator_types: &self.comparator_types,
                adt_types: &self.adt_types,
                main_ty: &self.main_ty,
                file: &self.file,
                source_root: &self.source_root,
            },
            flags: &self.flags,
            mono: MonoCtx {
                map: &self.mono_map,
                table: &self.mono_table,
            },
            poly_term_registry: &self.poly_term_registry,
            musttail_sink: None,
            unit_cost_sink: None,
            total_units: 1,
        }
    }
}

/// Build a `DefInfo` from its named fields (test shorthand). The module path
/// defaults to the owner unit's name as a single segment — override
/// `module_path` directly in tests that exercise path matching.
pub(in crate::compile::per_module) fn def_info(
    llvm_name: &str,
    ty: Type,
    owner_unit: &str,
) -> DefInfo {
    DefInfo {
        llvm_name: llvm_name.to_string(),
        ty,
        owner_unit: owner_unit.to_string(),
        module_path: vec![owner_unit.to_string()],
    }
}

/// Count `declare` lines for `@<symbol>(` in an IR dump.
pub(in crate::compile::per_module) fn count_declares_of(ir: &str, symbol: &str) -> usize {
    let needle = format!("@{symbol}(");
    ir.lines()
        .filter(|line| line.starts_with("declare") && line.contains(&needle))
        .count()
}
