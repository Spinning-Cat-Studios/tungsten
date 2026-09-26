//! Diagnostic rendering pipeline: deduplication, limiting, and summary output.

use super::hints::{self, HintTracker};
use super::renderers::{
    render_elab_error, render_elab_error_with_source_map, render_parse_error, render_warning,
    render_warning_with_source_map,
};
use crate::driver::modules::SourceMap;
use crate::{ElabError, ParseError};

mod dedup;
mod summary;

pub(super) use dedup::{deduplicate_errors, deduplicate_parse_errors};
use summary::print_summary;

/// Default source text and filename for diagnostic rendering.
pub struct SourceRef<'a> {
    pub source: &'a str,
    pub filename: &'a str,
}

/// How many diagnostics to render, and how many that withholds.
///
/// `max_errors` of 0 means no limit. Extracted from both render entry points
/// because it is the only arithmetic they do: inline, the `== 0` sentinel and
/// the saturating subtraction are reachable *only* by capturing stderr, which
/// is how a wrong limit would ship unnoticed.
fn display_budget(total: usize, max_errors: usize) -> (usize, usize) {
    let limit = if max_errors == 0 { total } else { max_errors };
    (limit, total.saturating_sub(limit))
}

/// How many of each diagnostic kind the budget covers.
///
/// Parse errors render first and so consume the budget first; elaboration
/// errors get what is left.
#[derive(Debug, PartialEq, Eq)]
struct RenderPlan {
    parse: usize,
    elab: usize,
}

/// Split a display budget across parse then elaboration errors.
///
/// Replaces a running counter compared against the limit inside both render
/// loops. The counter version was correct and unassertable — every comparison
/// in it survived mutation, because the only observable effect was which lines
/// reached stderr.
fn split_budget(parse_count: usize, elab_count: usize, limit: usize) -> RenderPlan {
    let parse = parse_count.min(limit);
    RenderPlan {
        parse,
        elab: elab_count.min(limit - parse),
    }
}

/// How many diagnostics were produced, before any deduplication.
///
/// The denominator of the summary's two-figure report (ADR 7.8.26d §2.3).
fn raw_diagnostic_count(parse_errors: &[ParseError], elab_errors: &[ElabError]) -> usize {
    parse_errors.len() + elab_errors.len()
}

/// Which deduplicated errors to render under a display budget
/// (ADR 14.8.26g D7): each failing file's first error, then the remaining
/// budget filled in list order. Returns ascending indices into `errors`.
///
/// The walk now reports every failing module in one run (D1), so a broken
/// project can produce hundreds of diagnostics and truncation is no longer
/// incidental. Taking the first `limit` errors would silently drop whole
/// files; reserving one slot per file keeps a truncated run saying how wide
/// the damage is. When there are more failing files than budget, the files
/// past the budget are omitted — no selection can do better.
fn truncation_plan(errors: &[&ElabError], limit: usize) -> Vec<usize> {
    if errors.len() <= limit {
        return (0..errors.len()).collect();
    }
    let mut chosen = vec![false; errors.len()];
    let mut budget = limit;
    let mut seen_files = std::collections::HashSet::new();
    for (i, error) in errors.iter().enumerate() {
        if budget == 0 {
            break;
        }
        if seen_files.insert(error.file_path.clone()) {
            chosen[i] = true;
            budget -= 1;
        }
    }
    for slot in &mut chosen {
        if budget == 0 {
            break;
        }
        if !*slot {
            *slot = true;
            budget -= 1;
        }
    }
    chosen
        .iter()
        .enumerate()
        .filter_map(|(i, taken)| taken.then_some(i))
        .collect()
}

/// Render elaboration errors with a maximum error limit.
///
/// `max_errors` of 0 means no limit.
pub fn render_diagnostics_with_source_map_limited(
    default: &SourceRef<'_>,
    source_map: &SourceMap,
    elab_errors: &[ElabError],
    warnings: &[ElabError],
    max_errors: usize,
) -> bool {
    // Deduplicate errors by span to reduce noise
    let deduped_errors = deduplicate_errors(elab_errors);

    // Limit the number of errors displayed, keeping at least one per failing
    // file (ADR 14.8.26g D7).
    let (limit, omitted_errors) = display_budget(deduped_errors.len(), max_errors);
    let errors_to_show: Vec<&ElabError> = truncation_plan(&deduped_errors, limit)
        .into_iter()
        .map(|i| deduped_errors[i])
        .collect();

    // Create hint tracker for dedup and category suppression
    let use_hints = hints::should_emit_hints();
    let mut hint_tracker = HintTracker::new();

    // Render elaboration errors
    for error in &errors_to_show {
        let tracker = if use_hints {
            Some(&mut hint_tracker)
        } else {
            None
        };
        render_elab_error_with_source_map(
            default.source,
            default.filename,
            source_map,
            error,
            tracker,
        );
    }

    // Render warnings (non-fatal, not counted in max_errors)
    for warning in warnings {
        render_warning_with_source_map(default.source, default.filename, source_map, warning);
    }

    print_summary(
        &hint_tracker,
        deduped_errors.len(),
        warnings.len(),
        omitted_errors,
        elab_errors.len(),
    )
}

/// Render elaboration errors, parse errors, and warnings with a maximum error limit.
///
/// `max_errors` of 0 means no limit.
pub fn render_diagnostics_limited(
    src: &SourceRef<'_>,
    elab_errors: &[ElabError],
    parse_errors: &[ParseError],
    warnings: &[ElabError],
    max_errors: usize,
) -> bool {
    // Deduplicate parse errors by span
    let deduped_parse = deduplicate_parse_errors(parse_errors);
    let deduped_elab = deduplicate_errors(elab_errors);

    // Calculate limits
    let total_count = deduped_parse.len() + deduped_elab.len();
    let (limit, omitted) = display_budget(total_count, max_errors);
    let plan = split_budget(deduped_parse.len(), deduped_elab.len(), limit);

    // Create hint tracker for dedup and category suppression
    let use_hints = hints::should_emit_hints();
    let mut hint_tracker = HintTracker::new();

    // Render parse errors first (up to limit)
    for error in deduped_parse.iter().take(plan.parse) {
        render_parse_error(src.source, src.filename, error);
    }

    // Render elaboration errors (remaining budget)
    for error in deduped_elab.iter().take(plan.elab) {
        let tracker = if use_hints {
            Some(&mut hint_tracker)
        } else {
            None
        };
        render_elab_error(src.source, src.filename, error, tracker, None);
    }

    // Render warnings (non-fatal, not counted in max_errors)
    for warning in warnings {
        render_warning(src.source, src.filename, warning);
    }

    print_summary(
        &hint_tracker,
        total_count,
        warnings.len(),
        omitted,
        raw_diagnostic_count(parse_errors, elab_errors),
    )
}

#[cfg(test)]
mod tests;
