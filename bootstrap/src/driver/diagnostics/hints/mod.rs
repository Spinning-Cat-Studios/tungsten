//! Compiler-embedded diagnostic hints (ADR 21.4.26h).
//!
//! Maps elaboration errors to contextual diagnostic command suggestions
//! that are appended to error output. This is the forcing function that
//! guides AI agents toward the diagnostic tool pipeline.

use crate::ElabErrorKind;
use std::cell::Cell;
use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::Path;

// ═══════════════════════════════════════════════════════════════════════
// Configuration
// ═══════════════════════════════════════════════════════════════════════

/// Controls when diagnostic hints are shown in error output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintMode {
    /// Auto-detect: on for TTY, off for non-TTY (default).
    Auto,
    /// Always show hints.
    On,
    /// Never show hints.
    Off,
}

thread_local! {
    static HINT_MODE: Cell<HintMode> = const { Cell::new(HintMode::Auto) };
}

/// Set the global hint mode.
pub fn set_hint_mode(mode: HintMode) {
    HINT_MODE.with(|m| m.set(mode));
}

/// Check whether hints should be emitted given the current mode.
pub fn should_emit_hints() -> bool {
    HINT_MODE.with(|m| match m.get() {
        HintMode::On => true,
        HintMode::Off => false,
        HintMode::Auto => std::io::stderr().is_terminal(),
    })
}

// ═══════════════════════════════════════════════════════════════════════
// Hint Categories
// ═══════════════════════════════════════════════════════════════════════

/// Categories for grouping errors and selecting diagnostic hints.
/// Maps from structured `ElabErrorKind` variants — no text matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HintCategory {
    TypeMismatch,
    NameResolution,
    PatternMatching,
    ControlFlow,
    Elaboration,
    EntryPoint,
    General,
}

impl HintCategory {
    /// Derive the hint category from an `ElabErrorKind`.
    pub fn from_error_kind(kind: &ElabErrorKind) -> Self {
        match kind {
            ElabErrorKind::TypeMismatch { .. }
            | ElabErrorKind::ExpectedFunction(_)
            | ElabErrorKind::ExpectedType { .. }
            | ElabErrorKind::ArityMismatch { .. }
            | ElabErrorKind::CannotInferType
            | ElabErrorKind::CannotInferTypeArg(_) => HintCategory::TypeMismatch,

            ElabErrorKind::UndefinedVariable(_)
            | ElabErrorKind::UndefinedType(_)
            | ElabErrorKind::UndefinedConstructor(_)
            | ElabErrorKind::DuplicateDefinition(_)
            | ElabErrorKind::ModuleNotFound { .. }
            | ElabErrorKind::ItemNotFoundInModule { .. }
            | ElabErrorKind::DuplicateImport { .. }
            | ElabErrorKind::GlobConflict { .. }
            | ElabErrorKind::UnresolvedImport(_)
            | ElabErrorKind::PrivateModule { .. }
            | ElabErrorKind::PrivateItem { .. }
            | ElabErrorKind::PublicItemLeak { .. } => HintCategory::NameResolution,

            ElabErrorKind::NonExhaustiveMatch
            | ElabErrorKind::UnreachableArm
            | ElabErrorKind::PatternTooDeep { .. }
            | ElabErrorKind::UnsupportedPattern(_) => HintCategory::PatternMatching,

            ElabErrorKind::DeadCodeAfterReturn => HintCategory::ControlFlow,

            ElabErrorKind::TryOnNonTryType(_)
            | ElabErrorKind::TryReturnMismatch { .. }
            | ElabErrorKind::TryOutsideReturnContext
            | ElabErrorKind::ReturnInsideTryBlock
            | ElabErrorKind::TryBlockRequiresResultType
            | ElabErrorKind::TryBlockExpectedSumEncoding
            | ElabErrorKind::TryBlockMissingConstructor(_) => HintCategory::TypeMismatch,

            ElabErrorKind::LetElseNonDiverging(_) => HintCategory::TypeMismatch,
            ElabErrorKind::LetElseIrrefutable => HintCategory::PatternMatching,
            ElabErrorKind::IfLetIrrefutable => HintCategory::PatternMatching,

            ElabErrorKind::NoMainFunction | ElabErrorKind::ContainsSorry => {
                HintCategory::EntryPoint
            }

            ElabErrorKind::UnsupportedFeature(_) | ElabErrorKind::MutabilityNotSupported => {
                HintCategory::Elaboration
            }

            ElabErrorKind::NotARecordType(_)
            | ElabErrorKind::MissingRecordField { .. }
            | ElabErrorKind::ExtraRecordField { .. }
            | ElabErrorKind::DuplicateRecordField(_) => HintCategory::TypeMismatch,

            ElabErrorKind::RecursiveAlias(_) => HintCategory::Elaboration,

            ElabErrorKind::ReflExpectedEquality(_)
            | ElabErrorKind::InvalidRefl { .. }
            | ElabErrorKind::SubstExpectedEquality(_)
            | ElabErrorKind::TransEndpointMismatch { .. }
            | ElabErrorKind::CongExpectedFunction(_)
            | ElabErrorKind::MotiveNotPredicate(_)
            | ElabErrorKind::MotiveDomainMismatch { .. }
            | ElabErrorKind::MotiveBodyNotType
            | ElabErrorKind::NatIndMotiveNotNat(_) => HintCategory::TypeMismatch,

            ElabErrorKind::Other(_) => HintCategory::General,
        }
    }

    /// Display name for use in `doctor suggest-tools` invocations.
    pub fn label(&self) -> &'static str {
        match self {
            HintCategory::TypeMismatch => "type mismatch",
            HintCategory::NameResolution => "name resolution",
            HintCategory::PatternMatching => "pattern matching",
            HintCategory::ControlFlow => "control flow",
            HintCategory::Elaboration => "elaboration error",
            HintCategory::EntryPoint => "entry point",
            HintCategory::General => "error",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Diagnostic Hints
// ═══════════════════════════════════════════════════════════════════════

/// A single diagnostic hint to append to error output.
#[derive(Debug, Clone)]
pub struct DiagnosticHint {
    /// The full command to run (e.g., `tungsten info type-encoding List examples/list.tg`).
    pub command: String,
    /// Brief reason why this command is useful.
    pub reason: String,
}

/// Select up to 2 diagnostic hints for an error.
///
/// Returns a specific hint (if applicable) plus the general `doctor suggest-tools`
/// entry point. Uses `ElabErrorKind` dispatch (no text matching on error messages).
pub fn select_hints(kind: &ElabErrorKind, file_path: Option<&Path>) -> Vec<DiagnosticHint> {
    let category = HintCategory::from_error_kind(kind);
    let file_arg = file_path
        .map(|p| shell_safe_path(p))
        .unwrap_or_else(|| "<file>".to_string());

    let mut hints = Vec::with_capacity(2);

    // First hint: specific to the error category
    match (kind, &category) {
        (ElabErrorKind::TypeMismatch { expected, .. }, _) => {
            // Try to extract a type name for the encoding command
            let type_name = extract_type_name(expected);
            hints.push(DiagnosticHint {
                command: format!(
                    "tungsten info type-encoding {} {}",
                    type_name.as_deref().unwrap_or("<Type>"),
                    file_arg
                ),
                reason: "Inspect type encoding to identify mismatch source".to_string(),
            });
        }
        (_, HintCategory::TypeMismatch) => {
            hints.push(DiagnosticHint {
                command: format!("tungsten explain error {}", kind.code()),
                reason: "Explain what this error code means".to_string(),
            });
        }
        (_, HintCategory::NameResolution) => {
            hints.push(DiagnosticHint {
                command: format!("tungsten explain error {}", kind.code()),
                reason: "Explain what this error code means".to_string(),
            });
        }
        (_, HintCategory::PatternMatching) => {
            hints.push(DiagnosticHint {
                command: format!("tungsten explain error {}", kind.code()),
                reason: "Explain what this error code means".to_string(),
            });
        }
        (_, HintCategory::Elaboration) => {
            hints.push(DiagnosticHint {
                command: format!("tungsten explain error {}", kind.code()),
                reason: "Explain what this error code means".to_string(),
            });
        }
        _ => {}
    }

    // Second hint: always the general suggest-tools entry point
    let suggest_cmd = format!("tungsten doctor suggest-tools \"{}\"", category.label());
    // Only add if different from the first hint
    if hints.iter().all(|h| h.command != suggest_cmd) {
        hints.push(DiagnosticHint {
            command: suggest_cmd,
            reason: "Get full list of diagnostic suggestions for this error class".to_string(),
        });
    }

    // Cap at 2
    hints.truncate(2);
    hints
}

// ═══════════════════════════════════════════════════════════════════════
// Hint Deduplication Tracker
// ═══════════════════════════════════════════════════════════════════════

/// Tracks which hint categories and commands have been emitted during a
/// compilation, enabling category-level suppression and command dedup.
#[derive(Debug, Default)]
pub struct HintTracker {
    /// Categories that have already shown hints.
    seen_categories: HashSet<HintCategory>,
    /// Individual commands that have been emitted.
    seen_commands: HashSet<String>,
    /// Count of suppressed hints.
    suppressed_count: usize,
}

impl HintTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter hints for an error: apply category-level suppression and command dedup.
    /// Returns the hints that should actually be shown.
    pub fn filter_hints(
        &mut self,
        kind: &ElabErrorKind,
        hints: Vec<DiagnosticHint>,
    ) -> Vec<DiagnosticHint> {
        let category = HintCategory::from_error_kind(kind);

        // Category-level suppression: only the first error per category gets hints
        if !self.seen_categories.insert(category) {
            self.suppressed_count += hints.len();
            return Vec::new();
        }

        // Command dedup: skip commands we've already shown
        let mut filtered = Vec::new();
        for hint in hints {
            if self.seen_commands.insert(hint.command.clone()) {
                filtered.push(hint);
            } else {
                self.suppressed_count += 1;
            }
        }
        filtered
    }

    /// Number of hints suppressed during this compilation.
    pub fn suppressed_count(&self) -> usize {
        self.suppressed_count
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Rendering
// ═══════════════════════════════════════════════════════════════════════

/// Format hints as lines for human-readable error output.
/// Each hint is prefixed with `  hint: run `.
pub fn format_hints(hints: &[DiagnosticHint]) -> String {
    let mut out = String::new();
    for hint in hints {
        out.push_str(&format!("  hint: run `{}`\n", hint.command));
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════
// JSON Output
// ═══════════════════════════════════════════════════════════════════════

/// A structured hint for JSON output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct JsonHint {
    pub command: String,
    pub reason: String,
}

/// A structured error with hints for JSON output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct JsonError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub hints: Vec<JsonHint>,
}

/// A structured JSON diagnostic report.
#[derive(Debug, Clone, serde::Serialize)]
pub struct JsonDiagnosticReport {
    pub errors: Vec<JsonError>,
}

/// Convert an ElabError to a JSON error with hints (always included in JSON mode).
pub fn error_to_json(error: &crate::ElabError, source: &str) -> JsonError {
    let hints = select_hints(&error.kind, error.file_path.as_deref());
    let line = compute_line_number(source, error.span.start as usize);

    JsonError {
        code: error.kind.code().to_string(),
        message: error.message.clone(),
        file: error.file_path.as_ref().map(|p| p.display().to_string()),
        line: Some(line),
        hints: hints
            .into_iter()
            .map(|h| JsonHint {
                command: h.command,
                reason: h.reason,
            })
            .collect(),
    }
}

/// Compute 1-based line number from byte offset.
fn compute_line_number(source: &str, offset: usize) -> u32 {
    let clamped = offset.min(source.len());
    source[..clamped].chars().filter(|&c| c == '\n').count() as u32 + 1
}

// ═══════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════

/// Make a file path safe for embedding in a shell command.
/// Single-quotes paths containing special characters.
fn shell_safe_path(path: &Path) -> String {
    let s = path.display().to_string();
    if s.contains(' ') || s.contains('\'') || s.contains('"') || s.contains('$') || s.contains('\\')
    {
        // Escape single quotes within the path, then wrap in single quotes
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s
    }
}

/// Try to extract a meaningful type name from a Type for use in hint commands.
/// Returns None if the type is too complex or anonymous.
fn extract_type_name(ty: &tungsten_core::Type) -> Option<String> {
    match ty {
        tungsten_core::Type::App(name, _) => Some(name.clone()),
        tungsten_core::Type::Adt(name, _, _) => Some(name.clone()),
        tungsten_core::Type::Mu(_, inner) => extract_type_name(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests_categories;
#[cfg(test)]
mod tests_helpers;
#[cfg(test)]
mod tests_selection;
