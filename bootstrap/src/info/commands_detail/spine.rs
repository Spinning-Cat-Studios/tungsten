//! `tungsten info type spine <type> <file>` — declared field count beside
//! encoded product-spine length (ADR 7.9.26c).
//!
//! A record of `n` fields encodes as a right-nested product, so field `i` is
//! `snd^i` then `fst` (ADR 1.8.26b D1). What that convention does NOT say is
//! what the spine is made *of*: a field whose encoding is itself a product is
//! **spliced** into the spine, so the spine can be longer than `n`, while a
//! field whose encoding is a reference (a named record, a generic
//! instantiation) is not. Both counts already exist on the elaborated project
//! — this prints them together so "does this fixture reach the shape?" is a
//! cost-3 question rather than a rebuild-and-compare.
//!
//! Two numbers, never a verdict (ADR 7.9.26c D2): what a gap between them means
//! depends on which defect is being hunted, and a verdict would go stale the
//! first time the question changed.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::driver::ProjectOutput;
use tungsten_core::types::Type;

use crate::info::elaborate_for_info;
use crate::info::helpers::format_type_short;

use super::diagnostic::print_type_not_found;

/// Print a record's declared field count beside its encoded spine length.
pub fn cmd_info_type_spine(
    name: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    let is_known = project.adt_types.contains_key(name)
        || project.record_types.contains_key(name)
        || project.type_aliases.contains_key(name);
    if !is_known {
        print_type_not_found(name, &project);
        return ExitCode::FAILURE;
    }

    match render_spine_report(name, &project) {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        // AC 5: a non-record or an uncached encoding says so on stderr and
        // exits non-zero, rather than printing a zero that reads like a count.
        Err(refusal) => {
            eprintln!("{refusal}");
            ExitCode::FAILURE
        }
    }
}

/// The right-nested components of a product spine: `(A × (B × C))` decomposes
/// to `[A, B, C]`.
///
/// The walk follows the **right** operand only, which is what makes it the
/// spine a field projection descends: a product in a `fst` slot (a non-final
/// field whose type is a tuple) stays one component, while a product in the
/// tail position continues the walk. That asymmetry is the whole reachability
/// condition of ADR 4.9.26b's defect.
pub(super) fn spine_components(ty: &Type) -> Vec<&Type> {
    let mut components = Vec::new();
    let mut cursor = ty;
    while let Type::Product(head, tail) = cursor {
        components.push(head.as_ref());
        cursor = tail.as_ref();
    }
    components.push(cursor);
    components
}

/// Build the report, or the reason there is none.
///
/// Separated from printing so tests can assert the exact output on fixtures
/// with known counts, and assert each refusal by its own message.
pub(super) fn render_spine_report(name: &str, project: &ProjectOutput) -> Result<String, String> {
    let Some(fields) = project.record_types.get(name) else {
        let kind = if project.adt_types.contains_key(name) {
            "an ADT"
        } else {
            "a type alias"
        };
        return Err(format!(
            "{name} is {kind}, not a record type — it has no declared fields to \
             compare a spine against."
        ));
    };

    let Some(encoded) = project.encoded_types.get(name) else {
        return Err(format!(
            "{name} is a record but has no cached encoding (parameterized records \
             encode per instantiation), so its spine cannot be measured here."
        ));
    };

    let components = spine_components(encoded);
    let mut report = String::new();
    let _ = writeln!(report, "Record Spine: {name}");
    let _ = writeln!(report, "{}", "\u{2550}".repeat(14 + name.len()));
    let _ = writeln!(report);
    let _ = writeln!(report, "Declared fields: {}", fields.len());
    let _ = writeln!(report, "Encoded spine:   {}", components.len());
    let _ = writeln!(report);
    render_declared_fields(&mut report, fields);
    render_spine_slots(&mut report, &components);
    let _ = writeln!(report, "{}", splice_summary(fields.len(), components.len()));
    Ok(report)
}

fn render_declared_fields(report: &mut String, fields: &[(String, Type)]) {
    let _ = writeln!(report, "Declared fields (source order):");
    for (i, (field_name, field_ty)) in fields.iter().enumerate() {
        let _ = writeln!(
            report,
            "  {i}: {field_name}: {}",
            format_type_short(field_ty)
        );
    }
    let _ = writeln!(report);
}

fn render_spine_slots(report: &mut String, components: &[&Type]) {
    let _ = writeln!(
        report,
        "Encoded spine components (right-nested; field i is snd^i then fst):"
    );
    for (i, component) in components.iter().enumerate() {
        let _ = writeln!(report, "  {i}: {}", format_type_short(component));
    }
    let _ = writeln!(report);
}

/// The one interpretive line, and it is structural rather than a verdict (D2):
/// it says what the two counts imply about the *encoding*, not whether any
/// particular defect is reachable.
fn splice_summary(field_count: usize, spine_len: usize) -> String {
    match spine_len.cmp(&field_count) {
        std::cmp::Ordering::Greater => format!(
            "Spine exceeds field count by {}: a field type that is structurally a \
             product is spliced into the spine rather than referenced.",
            spine_len - field_count
        ),
        std::cmp::Ordering::Equal => {
            "Spine equals field count: no field type is spliced into the spine.".to_string()
        }
        std::cmp::Ordering::Less => format!(
            "Spine is SHORTER than the field count by {} — the encoding does not \
             match the declaration; report this.",
            field_count - spine_len
        ),
    }
}

// Tests: spine_tests.rs
#[cfg(test)]
#[path = "spine_tests.rs"]
mod spine_tests;
