//! `tungsten info pipeline` — the canonical, AI-facing inventory of the
//! compiler's diagnostic tooling.
//!
//! The inventory is **data** ([`inventory::SECTIONS`]) reconciled against the
//! clap command tree by [`tests`], not prose (ADR 28.7.26f). Before that it was
//! fourteen `println!`-carrying functions with nothing asserting that a
//! registered subcommand appeared here, that an entry named a subcommand that
//! existed, or that an advertised cost tier was right — and it had drifted:
//! `doctor check link-collisions` was dispatched, recommended by
//! `suggest-tools`, and absent from this listing.
//!
//! To document a new tool, add a [`entry::PipelineEntry`] to the relevant
//! `inventory` module. Adding a subcommand *without* one fails the completeness
//! test; the escape hatch for a genuinely non-diagnostic command is
//! [`entry::EntryKind::NotDiagnostic`], which demands a stated reason.

pub mod entry;
mod inventory;
mod json;
mod render;

#[cfg(test)]
pub(crate) mod reconcile;
#[cfg(test)]
mod tests;

use std::process::ExitCode;

pub fn cmd_info_pipeline(json: bool) -> ExitCode {
    print!("{}", rendered_inventory(json));
    ExitCode::SUCCESS
}

/// Exactly the text [`cmd_info_pipeline`] prints — a value, so which surface a
/// flag selects is assertable rather than merely executed. The `print!` above
/// is then the only unobservable line left in this module, which is the point of
/// the whole ADR restated at its own entry point.
fn rendered_inventory(json: bool) -> String {
    if json {
        format!("{}\n", json::render_json(inventory::SECTIONS))
    } else {
        render::render(inventory::SECTIONS)
    }
}

/// The inventory's own one-line summary for the subcommand at `path`.
///
/// Exposed for tests outside this module: `explain error`'s entry makes the
/// same claim about the same withheld set as that command's help, so ADR
/// 19.8.26b checks the two together — the boundary is the claim, not the file.
#[cfg(test)]
pub(crate) fn entry_summary_for_test(path: &str) -> Option<&'static str> {
    inventory::SECTIONS
        .iter()
        .flat_map(|section| section.entries)
        .find(|entry| entry.path == path)
        .map(|entry| entry.summary)
}

#[cfg(test)]
mod entry_point_tests {
    use super::*;

    #[test]
    fn the_json_flag_selects_the_machine_readable_surface() {
        let emitted = rendered_inventory(true);
        let parsed: Vec<serde_json::Value> =
            serde_json::from_str(&emitted).expect("--json must emit valid JSON");
        assert!(!parsed.is_empty());
    }

    #[test]
    fn without_the_json_flag_the_human_listing_is_rendered() {
        let rendered = rendered_inventory(false);
        assert!(
            rendered.starts_with("Tungsten Compiler Pipeline"),
            "{rendered:.40}"
        );
        assert!(serde_json::from_str::<serde_json::Value>(&rendered).is_err());
    }

    #[test]
    fn both_surfaces_end_in_exactly_one_newline() {
        for json in [true, false] {
            let emitted = rendered_inventory(json);
            assert!(emitted.ends_with('\n'), "json={json}: no trailing newline");
            assert!(!emitted.ends_with("\n\n"), "json={json}: doubled newline");
        }
    }
}
