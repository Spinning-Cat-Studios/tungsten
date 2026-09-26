//! Where `--emit-llvm` puts each codegen unit's `.ll` file.
//!
//! **One derivation, two consumers.** The emitter ([`super::entry`]) and
//! `tungsten info codegen unit-paths` both need to know a unit's destination —
//! the emitter to write it, the diagnostic to report it and flag units that
//! would overwrite each other. A second copy of the rule would drift, and
//! text-vs-reality drift is precisely the failure the ADR 28.7.26e family kept
//! hitting. So the rule lives here and neither consumer restates it.
//!
//! The rule: a unit's `.ll` lands at
//! `<output_dir>/<source_file relative to source_root, extension stripped>/<def_llvm_name>.ll`.
//! The synthetic mono depot is the one exception — it has no source file and
//! lands at `<output_dir>/__mono.ll`.

use std::path::{Path, PathBuf};

/// A codegen unit's identity, reduced to what the destination depends on.
///
/// Deliberately NOT `&ModuleCodegenUnit`: taking the two fields the rule
/// actually reads is what lets the derivation be unit-tested on plain paths,
/// with no elaboration and no codegen.
pub(crate) struct UnitOrigin<'a> {
    /// The `.tg` file the unit's definition came from.
    pub(crate) source_file: &'a Path,
    /// The definition's **source** name — `def_llvm_name`'s `main` →
    /// `tungsten_main` rename is applied by this module, not by the caller.
    pub(crate) def_name: &'a str,
}

/// A unit whose source file lies outside the source root, so no mirror path
/// exists for it. The emitter treats this as a hard error; the diagnostic
/// reports it as an unplaceable unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutsideSourceRoot {
    pub(crate) source_file: PathBuf,
    pub(crate) source_root: PathBuf,
}

impl std::fmt::Display for OutsideSourceRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "source file '{}' is outside source root '{}'",
            self.source_file.display(),
            self.source_root.display()
        )
    }
}

/// The output directory `--emit-llvm` writes into (ADR 7.5.26h §2.3).
///
/// - `-o` naming a file (it has an extension) → that file's parent directory.
/// - `-o` naming a directory → that directory.
/// - neither → `target/ll/` beside the entry file.
pub(crate) fn resolve_emit_llvm_dir(file: &Path, output: Option<&Path>) -> PathBuf {
    match output {
        Some(p) if p.extension().is_some() => p.parent().unwrap_or(Path::new(".")).to_path_buf(),
        Some(p) => p.to_path_buf(),
        None => file
            .parent()
            .unwrap_or(Path::new("."))
            .join("target")
            .join("ll"),
    }
}

/// The `.ll` destination for one codegen unit.
pub(crate) fn emit_llvm_dest(
    origin: &UnitOrigin<'_>,
    source_root: &Path,
    output_dir: &Path,
) -> Result<PathBuf, OutsideSourceRoot> {
    let relative_dir = origin
        .source_file
        .strip_prefix(source_root)
        .map(|r| r.with_extension(""))
        .map_err(|_| OutsideSourceRoot {
            source_file: origin.source_file.to_path_buf(),
            source_root: source_root.to_path_buf(),
        })?;
    let def_name = crate::compile::def_llvm_name(origin.def_name);
    Ok(output_dir.join(relative_dir).join(format!("{def_name}.ll")))
}

/// The `.ll` destination for the synthetic monomorphization depot, which has no
/// source file and therefore no mirror directory.
pub(crate) fn mono_depot_dest(output_dir: &Path, unit_name: &str) -> PathBuf {
    output_dir.join(format!("{unit_name}.ll"))
}

#[cfg(test)]
mod tests;
