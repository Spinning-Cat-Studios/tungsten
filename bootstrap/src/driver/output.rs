//! Output formatting for values and types, and the project output type.

use std::collections::HashMap;

use crate::config::MAX_TYPE_DISPLAY_DEPTH;
use crate::elaborate::Constructor;
use tungsten_core::{Term, Type};

use super::modules::SourceMap;
use super::type_registry::{lookup_type_name, TYPE_PATTERN_REGISTRY};

/// Record type definitions: name -> fields.
/// Used by codegen to expand `TyVar("RecordName")` to structural product types.
pub type RecordTypes = HashMap<String, Vec<(String, Type)>>;

/// ADT type definitions: name -> (params, constructors).
/// Used by codegen to expand `Type::App("Name", args)` to sum/mu types.
pub type AdtTypes = HashMap<String, (Vec<String>, Vec<Constructor>)>;

/// Type alias definitions: name -> (params, target type).
/// Used by `info` commands for display.
pub type TypeAliases = HashMap<String, (Vec<String>, Type)>;

/// Optional trace filters for elaboration diagnostics.
#[derive(Clone, Default)]
pub struct TraceOptions {
    pub trace_types: Option<String>,
    pub trace_encoding: Option<String>,
    pub trace_normalization: Option<String>,
    pub trace_ctor_registration: bool,
    pub elab_mode: crate::elaborate::ElabMode,
}

/// Pipeline execution options: flags that control check/run behaviour.
pub struct PipelineOpts {
    pub mode: super::Mode,
    pub verbose: bool,
    pub dump_types: bool,
}

/// A codegen unit representing a single top-level function (ADR 9.5.26b).
///
/// Each codegen unit is emitted to a separate `.ll` file. The `source_file` path
/// is used to derive the mirror output path under `target/ll/`. Each unit contains
/// exactly one `CoreDef`; inner lambdas are co-located by the `CodeGen` layer.
#[derive(Debug, Clone)]
pub struct ModuleCodegenUnit {
    /// Logical module path segments (e.g., `["compiler", "lexer"]`)
    pub module_path: Vec<String>,
    /// Original `.tg` source file path (for mirror output path derivation)
    pub source_file: std::path::PathBuf,
    /// The single definition owned by this unit (exactly one element)
    pub defs: Vec<crate::elaborate::CoreDef>,
}

/// Complete output of project elaboration.
///
/// Aggregates elaboration results at the project level (driver output).
/// Unlike `ElabOutput` (per-module elaboration), this includes the source map
/// and omits warnings (which are rendered inline during elaboration).
pub struct ProjectOutput {
    /// The elaborated definitions (flat, for backward compatibility)
    pub defs: Vec<crate::elaborate::CoreDef>,
    /// Per-module codegen units (ADR 6.5.26c §2.1)
    pub codegen_units: Vec<ModuleCodegenUnit>,
    /// Record type definitions: name -> fields
    pub record_types: RecordTypes,
    /// ADT type definitions: name -> (params, constructors)
    pub adt_types: AdtTypes,
    /// Type alias definitions: name -> (params, target type)
    pub type_aliases: TypeAliases,
    /// Type provenance: μ-binder → ADT origin
    pub type_provenance: crate::elaborate::TypeProvenance,
    /// Source map for multi-file error reporting
    pub source_map: SourceMap,
    /// Cached type encodings from Phase 1e (ADR 20.4.26c)
    pub encoded_types: std::collections::HashMap<String, Type>,
    /// Mutual recursion groups from Phase 1c.5 SCC (ADR 20.4.26c)
    pub mutual_recursion_groups: std::collections::HashMap<String, Vec<String>>,
    /// Parent type visibilities (ADR 14.5.26c).
    /// Maps type name → declared visibility. Used by `info type visibility`.
    pub type_visibilities: std::collections::HashMap<String, crate::ast::Visibility>,
    /// Per-field visibility overrides for record types (ADR 14.5.26c).
    /// Maps record name → per-field visibility (None = inherit parent).
    pub record_field_visibilities:
        std::collections::HashMap<String, Vec<Option<crate::ast::Visibility>>>,
}

impl ProjectOutput {
    /// Build the set of known concrete type names (ADT ∪ record keys).
    ///
    /// Used by mono discovery and codegen to distinguish cross-module type
    /// references (`TyVar("Binding")`) from abstract type parameters (`TyVar("T")`).
    /// Type aliases are excluded: they are expanded during elaboration and do
    /// not appear as bare `TyVar` in Core IR bodies. See ADR 13.5.26a §2.2.
    pub fn concrete_type_names(&self) -> std::collections::HashSet<String> {
        self.adt_types
            .keys()
            .chain(self.record_types.keys())
            .cloned()
            .collect()
    }
}

/// Format a term as a human-readable value.
pub fn format_value(term: &Term) -> String {
    match term {
        Term::Zero => "0".to_string(),
        Term::Succ(n) => {
            if let Some(n) = term_to_nat(term) {
                n.to_string()
            } else {
                format!("succ({})", format_value(n))
            }
        }
        Term::True => "true".to_string(),
        Term::False => "false".to_string(),
        Term::Unit => "()".to_string(),
        Term::Pair(a, b) => format!("({}, {})", format_value(a), format_value(b)),
        Term::Inl(_, t) => format!("inl({})", format_value(t)),
        Term::Inr(_, t) => format!("inr({})", format_value(t)),
        Term::Lambda(_, _, _) | Term::Fix(_, _, _) => "<function>".to_string(),
        Term::TyAbs(_, _) => "<polymorphic function>".to_string(),
        Term::Refl(_, _) => "refl".to_string(),
        Term::Sorry => "sorry".to_string(),
        Term::StringLit(s) => format!("\"{}\"", s),
        Term::Fold(_, t) => format_value(t),
        _ => format!("{:?}", term),
    }
}

/// Try to convert a Term to a natural number.
fn term_to_nat(term: &Term) -> Option<u64> {
    match term {
        Term::Zero => Some(0),
        Term::Succ(n) => term_to_nat(n).map(|n| n + 1),
        _ => None,
    }
}

/// Format a type as a human-readable string.
pub fn format_type(ty: &Type) -> String {
    format_type_with_depth(ty, usize::MAX)
}

/// Format a type for display in error messages with depth-limited truncation.
///
/// Uses MAX_TYPE_DISPLAY_DEPTH to control how deeply nested types are expanded.
/// Types exceeding the depth limit are truncated with "..." to keep error
/// messages readable.
///
/// For non-parameterized user-defined types (ADTs, Records, Aliases), attempts
/// to show the original type name instead of the Core encoding.
pub fn format_type_for_display(ty: &Type) -> String {
    // First, try to find a user-defined type name for this encoding.
    // This handles non-parameterized types like `Color`, `Direction`, etc.
    if let Some(name) = lookup_type_name(ty) {
        return name;
    }

    format_type_with_depth(ty, MAX_TYPE_DISPLAY_DEPTH)
}

/// Format a type with explicit depth limit.
///
/// When depth reaches 0, nested structures are shown as "...".
fn format_type_with_depth(ty: &Type, depth: usize) -> String {
    if depth == 0 {
        return "...".to_string();
    }

    // Base types (no recursion needed)
    if let Some(s) = format_base_type(ty) {
        return s;
    }

    match ty {
        // Binary compound types
        Type::Arrow(a, b) => {
            let a_str = if matches!(**a, Type::Arrow(_, _)) {
                format!("({})", format_type_with_depth(a, depth - 1))
            } else {
                format_type_with_depth(a, depth - 1)
            };
            format!("{} -> {}", a_str, format_type_with_depth(b, depth - 1))
        }
        Type::Product(a, b) | Type::Sum(a, b) => {
            let op = if matches!(ty, Type::Product(..)) {
                " × "
            } else {
                " + "
            };
            format!(
                "({}{}{})",
                format_type_with_depth(a, depth - 1),
                op,
                format_type_with_depth(b, depth - 1)
            )
        }

        // Binding forms
        Type::Forall(name, body) => {
            format!("∀ {}. {}", name, format_type_with_depth(body, depth - 1))
        }
        Type::Mu(name, body) => format_mu_type(name, body, depth),

        // Parameterized wrappers
        Type::Ptr(inner) | Type::Ref(inner) => {
            let tag = if matches!(ty, Type::Ptr(_)) {
                "Ptr"
            } else {
                "Ref"
            };
            format!("{}<{}>", tag, format_type_with_depth(inner, depth - 1))
        }

        // Equality proofs
        Type::Eq(inner_ty, a, b) => format_eq_with_depth(inner_ty, a, b, depth),

        // Type applications / ADT — both use name<args> format
        Type::App(name, args) | Type::Adt(name, args, _) => {
            format_app_with_depth(name, args, depth)
        }

        // Base types already handled above
        _ => unreachable!(),
    }
}

/// Format base (leaf) types that require no recursion.
fn format_base_type(ty: &Type) -> Option<String> {
    match ty {
        Type::Unit => Some("Unit".to_string()),
        Type::Void => Some("Void".to_string()),
        Type::Bool => Some("Bool".to_string()),
        Type::Nat => Some("Nat".to_string()),
        Type::Prop => Some("Prop".to_string()),
        Type::String => Some("String".to_string()),
        Type::Error => Some("<error>".to_string()),
        Type::TyVar(name) => Some(name.strip_prefix('@').unwrap_or(name).to_string()),
        _ => None,
    }
}

fn format_eq_with_depth(inner_ty: &Type, a: &Term, b: &Term, depth: usize) -> String {
    if depth <= 1 {
        format!("Eq<{}, ...>", format_type_with_depth(inner_ty, depth - 1))
    } else {
        format!(
            "Eq<{}, {:?}, {:?}>",
            format_type_with_depth(inner_ty, depth - 1),
            a,
            b
        )
    }
}

fn format_app_with_depth(name: &str, args: &[Type], depth: usize) -> String {
    if args.is_empty() {
        name.to_string()
    } else {
        let args_str: Vec<String> = args
            .iter()
            .map(|a| format_type_with_depth(a, depth - 1))
            .collect();
        format!("{}<{}>", name, args_str.join(", "))
    }
}

fn format_mu_type(name: &str, body: &Type, depth: usize) -> String {
    // Try to extract a readable type name from our encoding convention.
    // We use names like "α_List" or "α_Tree" for recursive types.
    if let Some(type_name) = name.strip_prefix("α_") {
        let has_params = TYPE_PATTERN_REGISTRY.with(|registry| {
            registry
                .borrow()
                .iter()
                .find(|p| p.name == type_name)
                .map(|p| !p.params.is_empty())
                .unwrap_or(false)
        });

        if has_params {
            format!("{}<...>", type_name)
        } else {
            type_name.to_string()
        }
    } else {
        format!("μ{}. {}", name, format_type_with_depth(body, depth - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_core::types::Type;

    #[test]
    fn test_format_base_type_returns_some_for_base_types() {
        assert_eq!(format_base_type(&Type::Bool), Some("Bool".to_string()));
        assert_eq!(format_base_type(&Type::Nat), Some("Nat".to_string()));
        assert_eq!(format_base_type(&Type::Unit), Some("Unit".to_string()));
        assert_eq!(format_base_type(&Type::String), Some("String".to_string()));
        assert_eq!(format_base_type(&Type::Error), Some("<error>".to_string()));
    }

    #[test]
    fn test_format_base_type_returns_none_for_compound_types() {
        assert_eq!(format_base_type(&Type::arrow(Type::Nat, Type::Bool)), None);
        assert_eq!(
            format_base_type(&Type::product(Type::Nat, Type::Bool)),
            None
        );
    }

    #[test]
    fn test_format_base_type_strips_at_prefix() {
        assert_eq!(
            format_base_type(&Type::TyVar("@T".to_string())),
            Some("T".to_string())
        );
        assert_eq!(
            format_base_type(&Type::TyVar("T".to_string())),
            Some("T".to_string())
        );
    }

    #[test]
    fn test_format_type_delegates_to_depth() {
        // format_type should produce the same output as format_type_with_depth(_, MAX)
        let ty = Type::arrow(Type::Nat, Type::Bool);
        assert_eq!(format_type(&ty), "Nat -> Bool");
    }

    #[test]
    fn test_format_type_arrow_precedence() {
        let ty = Type::arrow(Type::arrow(Type::Nat, Type::Bool), Type::Unit);
        assert_eq!(format_type(&ty), "(Nat -> Bool) -> Unit");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Codegen unit construction
// ═══════════════════════════════════════════════════════════════════════

/// Build one `ModuleCodegenUnit` per `CoreDef` from per-module definition groups.
///
/// Each codegen unit wraps a single definition — codegen emits one `.ll` file
/// per unit. The module path and source file are propagated from the per-module
/// elaboration output.
pub(crate) fn build_codegen_units(
    module_defs: Vec<(
        Vec<String>,
        std::path::PathBuf,
        Vec<crate::elaborate::CoreDef>,
    )>,
) -> Vec<ModuleCodegenUnit> {
    module_defs
        .into_iter()
        .flat_map(|(module_path, source_file, defs)| {
            defs.into_iter().map(move |def| ModuleCodegenUnit {
                module_path: module_path.clone(),
                source_file: source_file.clone(),
                defs: vec![def],
            })
        })
        .collect()
}
