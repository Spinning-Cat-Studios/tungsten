//! Default messages for elaboration error kinds.
//!
//! Split from `kind.rs` to keep that file under the 400-line limit.

use super::kind::ElabErrorKind;

impl ElabErrorKind {
    /// Get the default message for this error kind.
    pub(in crate::elaborate) fn default_message(&self) -> String {
        match self {
            // Name resolution — simple "not found" errors
            ElabErrorKind::UndefinedVariable(name) => {
                format!("cannot find value `{}` in this scope", name)
            }
            ElabErrorKind::UndefinedType(name) => {
                format!("cannot find type `{}` in this scope", name)
            }
            ElabErrorKind::UndefinedConstructor(name) => {
                format!("cannot find constructor `{}` in this scope", name)
            }
            ElabErrorKind::DuplicateDefinition(name) => {
                format!("the name `{}` is defined multiple times", name)
            }

            // Module/import errors (delegated — contain internal branching)
            ElabErrorKind::ModuleNotFound { .. }
            | ElabErrorKind::ItemNotFoundInModule { .. }
            | ElabErrorKind::DuplicateImport { .. }
            | ElabErrorKind::GlobConflict { .. }
            | ElabErrorKind::UnresolvedImport(_)
            | ElabErrorKind::PrivateModule { .. }
            | ElabErrorKind::PrivateItem { .. }
            | ElabErrorKind::PublicItemLeak { .. } => self.format_module_message(),

            // Type errors
            ElabErrorKind::TypeMismatch { expected, found } => {
                format!("expected `{}`, found `{}`", expected, found)
            }
            ElabErrorKind::CannotInferType => {
                "cannot infer type; add a type annotation".to_string()
            }
            ElabErrorKind::CannotInferTypeArg(var) => {
                format!(
                    "cannot infer type argument `{}`; provide explicit type arguments",
                    var
                )
            }
            ElabErrorKind::ArityMismatch { expected, found } => {
                format!("expected {} arguments, found {}", expected, found)
            }
            ElabErrorKind::ExpectedFunction(ty) => {
                format!("expected function, found `{}`", ty)
            }
            ElabErrorKind::ExpectedType { expected, found } => {
                format!("expected `{}`, found `{}`", expected, found)
            }

            // Phase 1 restrictions
            ElabErrorKind::UnsupportedFeature(feature) => {
                format!("`{}` is not supported", feature)
            }
            ElabErrorKind::MutabilityNotSupported => {
                "mutable bindings are not supported; use shadowing instead".to_string()
            }

            // Pattern matching errors
            ElabErrorKind::NonExhaustiveMatch => "non-exhaustive match patterns".to_string(),
            ElabErrorKind::UnreachableArm => "unreachable pattern".to_string(),
            ElabErrorKind::DeadCodeAfterReturn => "unreachable code after `return`".to_string(),
            ElabErrorKind::PatternTooDeep { depth, max } => {
                format!("pattern nesting depth {} exceeds maximum of {}", depth, max)
            }
            ElabErrorKind::UnsupportedPattern(pat) => {
                format!("pattern `{}` is not supported", pat)
            }

            // Try operator errors
            ElabErrorKind::TryOnNonTryType(ty) => {
                format!(
                    "`?` operator requires `Result<T, E>` or `Option<T>`, found `{}`",
                    ty
                )
            }
            ElabErrorKind::TryReturnMismatch {
                operand_type,
                return_type,
            } => {
                format!(
                    "cannot use `?` on `{}` in function returning `{}`",
                    operand_type, return_type
                )
            }
            ElabErrorKind::TryOutsideReturnContext => {
                "`?` can only be used inside a function or closure body with a known return type"
                    .to_string()
            }
            ElabErrorKind::ReturnInsideTryBlock => {
                "`return` inside `try` block is not allowed; use `?` to propagate errors"
                    .to_string()
            }
            ElabErrorKind::TryBlockRequiresResultType => {
                "`try` block requires a `Result` type in scope; add a type annotation".to_string()
            }
            ElabErrorKind::TryBlockExpectedSumEncoding => {
                "expected `Result` type (Sum encoding) for `try` block".to_string()
            }
            ElabErrorKind::TryBlockMissingConstructor(name) => {
                format!(
                    "`Result` type must have `{}` constructor for `try` block",
                    name
                )
            }

            // Let-else errors
            ElabErrorKind::LetElseNonDiverging(ty) => {
                format!(
                    "`else` branch in `let`-`else` must diverge (e.g., `return`), found type `{}`",
                    ty
                )
            }
            ElabErrorKind::LetElseIrrefutable => {
                "irrefutable pattern in `let`-`else`; `else` branch is unreachable".to_string()
            }
            ElabErrorKind::IfLetIrrefutable => {
                "irrefutable pattern in `if let`; condition always matches".to_string()
            }

            // Named record errors
            ElabErrorKind::NotARecordType(name) => {
                format!("`{}` is not a record type", name)
            }
            ElabErrorKind::MissingRecordField { field, type_name } => {
                format!("missing field `{}` for record type `{}`", field, type_name)
            }
            ElabErrorKind::ExtraRecordField { field, type_name } => {
                format!("unknown field `{}` for record type `{}`", field, type_name)
            }
            ElabErrorKind::DuplicateRecordField(name) => {
                format!("duplicate field `{}`", name)
            }

            // Entry point errors
            ElabErrorKind::NoMainFunction => "no `main` function found".to_string(),
            ElabErrorKind::ContainsSorry => "cannot compile file containing `sorry`".to_string(),
            ElabErrorKind::RecursiveAlias(name) => {
                format!("recursive type alias `{}` references itself", name)
            }

            // Equality proof errors (ADR 21.5.26d) — delegated
            ElabErrorKind::ReflExpectedEquality(_)
            | ElabErrorKind::InvalidRefl { .. }
            | ElabErrorKind::SubstExpectedEquality(_)
            | ElabErrorKind::TransEndpointMismatch { .. }
            | ElabErrorKind::CongExpectedFunction(_) => self.format_equality_message(),

            // Motive errors (ADR 21.5.26g)
            ElabErrorKind::MotiveNotPredicate(ty) => {
                format!("`subst` motive must be a predicate lambda `|x: τ| <type>`, but found type `{}`", ty)
            }
            ElabErrorKind::MotiveDomainMismatch { expected, found } => {
                format!(
                    "motive parameter type `{}` does not match equality base type `{}`",
                    found, expected
                )
            }
            ElabErrorKind::MotiveBodyNotType => {
                "motive body must be a type expression, not a term".to_string()
            }

            ElabErrorKind::NatIndMotiveNotNat(found) => {
                format!(
                    "`natind` motive domain must be `Nat`, but found `{}`",
                    found
                )
            }

            ElabErrorKind::Other(msg) => msg.clone(),
        }
    }

    /// Format messages for equality proof error kinds (ADR 21.5.26d).
    fn format_equality_message(&self) -> String {
        match self {
            ElabErrorKind::ReflExpectedEquality(ty) => {
                format!("`refl` can only be checked against an equality type, but the expected type was `{}`", ty)
            }
            ElabErrorKind::InvalidRefl { left, right } => {
                format!(
                    "`refl` requires both sides to be equal, but found `{}` and `{}`",
                    left, right
                )
            }
            ElabErrorKind::SubstExpectedEquality(ty) => {
                format!(
                    "`subst` proof argument must have an equality type, but found `{}`",
                    ty
                )
            }
            ElabErrorKind::TransEndpointMismatch { left, right } => {
                format!("`trans` endpoint mismatch: first proof ends with `{}`, second starts with `{}`", left, right)
            }
            ElabErrorKind::CongExpectedFunction(ty) => {
                format!(
                    "`cong` first argument must be a function, but found type `{}`",
                    ty
                )
            }
            _ => unreachable!("format_equality_message called with non-equality error"),
        }
    }

    /// Format messages for module/import error kinds.
    fn format_module_message(&self) -> String {
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
