//! Default messages for the module/import error kinds.
//!
//! Split from `messages.rs` when ADR 14.9.26c's two `Int` diagnostics took
//! it past the file-size cap — on the seam `constructors_modules.rs` already
//! uses, so the module-system half of the error surface reads as one pair.

use super::kind::ElabErrorKind;

impl ElabErrorKind {
    /// Format messages for module/import error kinds.
    pub(super) fn format_module_message(&self) -> String {
        match self {
            ElabErrorKind::ModuleNotFound { module, suggestion } => {
                if let Some(s) = suggestion {
                    format!("cannot find module `{}`; did you mean `{}`?", module, s)
                } else {
                    format!("cannot find module `{}`", module)
                }
            }
            ElabErrorKind::ItemNotFoundInModule { module, item } => {
                format!("cannot find `{}` in module `{}`", item, module)
            }
            ElabErrorKind::DuplicateImport {
                name,
                first_source_module,
                second_source_module,
                ..
            } => {
                if first_source_module == second_source_module {
                    format!("the name `{}` is imported multiple times", name)
                } else {
                    format!(
                        "the name `{}` is imported from both `{}` and `{}`",
                        name, first_source_module, second_source_module
                    )
                }
            }
            ElabErrorKind::GlobConflict {
                name,
                first_module,
                second_module,
            } => {
                format!(
                    "`{}` is imported from both `{}::*` and `{}::*`",
                    name, first_module, second_module
                )
            }
            ElabErrorKind::UnresolvedImport(path) => {
                format!("cannot resolve import `{}`", path)
            }
            ElabErrorKind::PrivateModule {
                module_path,
                accessed_from,
            } => {
                format!(
                    "module `{}` is private and cannot be accessed from `{}`",
                    module_path, accessed_from
                )
            }
            ElabErrorKind::PrivateItem {
                item_name,
                item_kind,
                defined_in,
                accessed_from,
            } => {
                format!(
                    "{} `{}` is private (defined in `{}`) and cannot be accessed from `{}`",
                    item_kind, item_name, defined_in, accessed_from
                )
            }
            ElabErrorKind::PublicItemLeak {
                item_name,
                item_kind,
                required_visibility,
                leak_path,
                leaked_visibility,
            } => {
                let path_str = leak_path.join(" -> ");
                format!(
                    "{} {} `{}` exposes {} type `{}` in its signature (via: {})",
                    required_visibility,
                    item_kind,
                    item_name,
                    leaked_visibility,
                    leak_path.last().unwrap_or(&item_name.clone()),
                    path_str
                )
            }
            _ => unreachable!("format_module_message called with non-module error"),
        }
    }
}
