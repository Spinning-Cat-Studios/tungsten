//! Rendering for the `tungsten info pipeline` inventory.
//!
//! Cosmetic by design (ADR 28.7.26f D1): every fact lives in
//! [`super::entry::PipelineEntry`], and this module only decides where it lands
//! on the line. The `[cost N]` and `[requires codegen]` markers are *derived*
//! here from the entry's fields rather than typed into its prose, so a wrong
//! tier is a failing test rather than a stale sentence.

use std::fmt::Write;

use super::entry::{EntryKind, PipelineEntry, Section};

/// Column at which an entry's summary starts, and at which its continuation
/// lines are indented.
const SUMMARY_COL: usize = 39;

/// Width of the left-hand stage label column in a flag table ("Elaborate:").
const FLAG_GROUP_WIDTH: usize = 12;

/// The cost ladder, restated at the foot of the listing — it is the reason the
/// inventory exists, so it is the last thing a reader sees.
const COST_SCALE_FOOTER: &str =
    "\nCost scale: 1=instant  2=parse  3=elaborate  4=compile  5=compile+run\n\
     Lower is faster. Start at cost 1; escalate as needed.";

/// Render the whole inventory as the human-readable listing.
pub fn render(sections: &[Section]) -> String {
    let mut out = String::new();
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        render_section(&mut out, section);
    }
    out.push_str(COST_SCALE_FOOTER);
    out.push('\n');
    out
}

fn render_section(out: &mut String, section: &Section) {
    if !section.title.is_empty() {
        if section.cost_hint.is_empty() {
            let _ = writeln!(out, "{}:", section.title);
        } else {
            let _ = writeln!(out, "{} [{}]:", section.title, section.cost_hint);
        }
    }
    let is_flag_table = section.entries.iter().any(|e| !e.group.is_empty());
    for entry in section.entries {
        render_entry(out, entry, section, is_flag_table);
    }
}

fn render_entry(out: &mut String, entry: &PipelineEntry, section: &Section, is_flag_table: bool) {
    // Verbatim only for a *prose block*: a Note that also carries a usage (a
    // recipe line, say) still wants the two-column layout, so both conditions
    // are load-bearing.
    if entry.kind == EntryKind::Note && entry.usage.is_empty() {
        out.push_str(entry.summary);
        out.push('\n');
        return;
    }
    let left = if is_flag_table {
        format!(
            "  {:<width$}{}",
            entry.group,
            entry.usage,
            width = FLAG_GROUP_WIDTH
        )
    } else {
        format!("  {}", entry.usage)
    };
    let body = summary_lines(entry, section);
    let (first, rest) = body
        .split_first()
        .expect("summary_lines primes at least one line");
    // Strictly less: a left column reaching the summary column exactly would
    // leave the summary abutting the usage with no separating space.
    if left.chars().count() < SUMMARY_COL {
        let _ = writeln!(out, "{left:<SUMMARY_COL$}{first}");
    } else {
        let _ = writeln!(out, "{left}");
        let _ = writeln!(out, "{:SUMMARY_COL$}{first}", "");
    }
    for line in rest {
        let _ = writeln!(out, "{:SUMMARY_COL$}{line}", "");
    }
}

/// The entry's prose, with the derived cost / codegen marker appended to its
/// last line and any `see also` cross-references on a line of their own.
fn summary_lines(entry: &PipelineEntry, section: &Section) -> Vec<String> {
    let mut lines: Vec<String> = entry.summary.lines().map(str::to_string).collect();
    if lines.is_empty() {
        lines.push(String::new());
    }
    let marker = derived_marker(entry, section);
    if !marker.is_empty() {
        let last = lines.last_mut().expect("primed with at least one line");
        let _ = write!(last, "  {marker}");
    }
    if !entry.see_also.is_empty() {
        lines.push(format!("See also: {}", entry.see_also.join(", ")));
    }
    lines
}

/// `[cost 4, requires codegen]` — derived, never hand-typed. A tier equal to the
/// section's default is left implicit, exactly as the hand-written listing did.
fn derived_marker(entry: &PipelineEntry, section: &Section) -> String {
    let mut marks: Vec<String> = Vec::new();
    if entry.cost != section.default_cost {
        if let Some(cost) = entry.cost {
            marks.push(format!("cost {}", cost.tier()));
        }
    }
    if entry.requires_codegen {
        marks.push("requires codegen".to_string());
    }
    if marks.is_empty() {
        String::new()
    } else {
        format!("[{}]", marks.join(", "))
    }
}

#[cfg(test)]
mod tests;
