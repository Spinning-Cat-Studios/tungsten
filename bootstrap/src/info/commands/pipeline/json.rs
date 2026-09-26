//! `tungsten info pipeline --json` — the inventory as machine-readable data.
//!
//! Mirrors `tungsten commands --json`. The point (ADR 28.7.26f D4) is that a
//! future check, hook, or agent tool reads the inventory as data rather than
//! scraping the rendered text, which is how the two-representations-one-fact
//! drift class reproduces itself.

use serde_json::{json, Value};

use super::entry::{EntryKind, PipelineEntry, Section};

/// Every entry as a flat array of objects, each tagged with its section.
///
/// Flat rather than nested by section for the same reason `tungsten commands
/// --json` is flat: a consumer looking up one tool should not have to walk a
/// tree to find it. Every key is always present — `path` is empty and `cost`
/// null for prose — so a consumer can key on shape rather than probing.
pub fn render_json(sections: &[Section]) -> String {
    let entries: Vec<Value> = sections
        .iter()
        .flat_map(|section| {
            section
                .entries
                .iter()
                .map(move |entry| entry_json(section, entry))
        })
        .collect();
    serde_json::to_string_pretty(&entries).expect("inventory entries are plain JSON values")
}

fn entry_json(section: &Section, entry: &PipelineEntry) -> Value {
    json!({
        "section": section.title,
        "path": entry.path,
        "usage": entry.usage,
        "kind": kind_name(entry.kind),
        "why_not_diagnostic": match entry.kind {
            EntryKind::NotDiagnostic { why } => Value::String(why.to_string()),
            _ => Value::Null,
        },
        "flag_on": match entry.kind {
            EntryKind::Flag { on } => Value::String(on.to_string()),
            _ => Value::Null,
        },
        "cost": entry.cost.map_or(Value::Null, |c| json!(c.tier())),
        "requires_codegen": entry.requires_codegen,
        "summary": entry.summary,
        "see_also": entry.see_also,
    })
}

const fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Subcommand => "subcommand",
        EntryKind::Flag { .. } => "flag",
        EntryKind::MakeTarget => "make-target",
        EntryKind::EnvVar => "env-var",
        EntryKind::Note => "note",
        EntryKind::NotDiagnostic { .. } => "not-diagnostic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::info::commands::pipeline::entry::CostTier;

    const SECTION: Section = Section {
        title: "Info commands",
        cost_hint: "cost 3",
        default_cost: Some(CostTier::Elaborate),
        entries: &[],
    };

    fn json_for(entry: PipelineEntry) -> Value {
        let section = Section {
            entries: Box::leak(Box::new([entry])),
            ..SECTION
        };
        let parsed: Vec<Value> = serde_json::from_str(&render_json(&[section])).unwrap();
        parsed.into_iter().next().unwrap()
    }

    #[test]
    fn a_subcommand_carries_its_path_kind_and_numeric_cost() {
        let value = json_for(
            PipelineEntry::subcommand("info def", "tungsten info def <n> <f>", "Show a def")
                .with_cost(CostTier::Elaborate),
        );
        assert_eq!(value["path"], "info def");
        assert_eq!(value["kind"], "subcommand");
        assert_eq!(value["cost"], 3);
        assert_eq!(value["section"], "Info commands");
        assert_eq!(value["requires_codegen"], false);
        assert_eq!(value["why_not_diagnostic"], Value::Null);
    }

    #[test]
    fn a_note_carries_null_cost_and_an_empty_path_rather_than_missing_keys() {
        let value = json_for(PipelineEntry::note("some prose"));
        assert_eq!(value["kind"], "note");
        assert_eq!(value["path"], "");
        assert_eq!(value["cost"], Value::Null);
        assert!(value.get("cost").is_some(), "key must be present");
    }

    #[test]
    fn a_not_diagnostic_entry_carries_its_stated_reason() {
        let value = json_for(PipelineEntry::not_diagnostic(
            "repl",
            "core workflow command",
        ));
        assert_eq!(value["kind"], "not-diagnostic");
        assert_eq!(value["why_not_diagnostic"], "core workflow command");
    }

    #[test]
    fn codegen_only_entries_are_flagged_and_see_also_survives() {
        let value = json_for(
            PipelineEntry::subcommand("info type lowering", "tungsten info type lowering", "L")
                .with_cost(CostTier::Compile)
                .requiring_codegen()
                .with_see_also(&["doctor check type lowering-consistency"]),
        );
        assert_eq!(value["requires_codegen"], true);
        assert_eq!(value["cost"], 4);
        assert_eq!(
            value["see_also"],
            json!(["doctor check type lowering-consistency"])
        );
    }

    #[test]
    fn a_flag_entry_names_the_command_that_accepts_it() {
        let value = json_for(PipelineEntry::flag_on(
            "check",
            "--json (check only)",
            "Emit JSON diagnostics",
        ));
        assert_eq!(value["kind"], "flag");
        assert_eq!(value["flag_on"], "check");
    }

    #[test]
    fn a_global_flag_records_an_empty_owner_rather_than_null() {
        let value = json_for(PipelineEntry::global_flag("--hints", "Force hints on"));
        assert_eq!(value["flag_on"], "");
    }

    #[test]
    fn a_non_flag_entry_carries_a_null_owner() {
        let value = json_for(PipelineEntry::note("prose"));
        assert_eq!(value["flag_on"], Value::Null);
    }

    #[test]
    fn every_entry_kind_has_a_distinct_stable_name() {
        let names = [
            kind_name(EntryKind::Subcommand),
            kind_name(EntryKind::Flag { on: "compile" }),
            kind_name(EntryKind::MakeTarget),
            kind_name(EntryKind::EnvVar),
            kind_name(EntryKind::Note),
            kind_name(EntryKind::NotDiagnostic { why: "x" }),
        ];
        let mut unique = names.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            names.len(),
            "duplicate kind name in {names:?}"
        );
    }
}
