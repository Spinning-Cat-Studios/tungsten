//! The `tungsten info pipeline` inventory — one central `&[Section]`.
//!
//! Central rather than colocated beside each clap definition (ADR 28.7.26f D6):
//! in Rust, colocation needs either a central array naming every `const` — which
//! is a central list again — or a distributed-slice dependency. What makes a
//! distant table safe is not proximity but the reconciliation test in
//! [`super::tests`], which fails the build when a subcommand has no
//! classification. The measure of success is that the test never needs a manual
//! allowlist.

mod doctor_commands;
mod doctor_ir_audits;
mod explain_commands;
mod info_commands;
mod notes;
mod overview;
mod tooling;

// Re-bound here (rather than each section module reaching two levels up) so the
// data modules import at depth 1 — the repo's nested-super convention.
use super::entry::{CostTier, PipelineEntry, Section};

/// Every section of the listing, in render order.
pub const SECTIONS: &[Section] = &[
    Section {
        title: "",
        cost_hint: "",
        default_cost: None,
        entries: overview::PIPELINE_BANNER,
    },
    Section {
        title: "Compile",
        cost_hint: "cost 4 — full codegen + link",
        default_cost: Some(CostTier::Compile),
        entries: overview::COMPILE_COMMAND,
    },
    Section {
        title: "Diagnostic flags (on `tungsten compile`)",
        cost_hint: "",
        default_cost: None,
        entries: overview::COMPILE_FLAGS,
    },
    Section {
        title: "Global flags",
        cost_hint: "",
        default_cost: None,
        entries: overview::GLOBAL_FLAGS,
    },
    Section {
        title: "Info commands",
        cost_hint: "cost 3 — parse + elaborate",
        default_cost: Some(CostTier::Elaborate),
        entries: info_commands::INFO_COMMANDS,
    },
    Section {
        title: "Explain commands",
        cost_hint: "cost 1 — instant, no file I/O",
        default_cost: Some(CostTier::Instant),
        entries: explain_commands::EXPLAIN_COMMANDS,
    },
    Section {
        title: "Health check commands",
        cost_hint: "cost 3 — parse + elaborate",
        default_cost: Some(CostTier::Elaborate),
        entries: doctor_commands::DOCTOR_COMMANDS,
    },
    Section {
        title: "IR audits over emitted .ll",
        cost_hint: "cost 1 — text scan",
        default_cost: Some(CostTier::Instant),
        entries: doctor_ir_audits::DOCTOR_IR_AUDITS,
    },
    Section {
        title: "Project health checks",
        cost_hint: "cost 3 — parse + elaborate",
        default_cost: Some(CostTier::Elaborate),
        entries: doctor_commands::DOCTOR_PROJECT_CHECKS,
    },
    Section {
        title: "GDB debugging",
        cost_hint: "cost 4 — requires compiled binary + devcontainer",
        default_cost: Some(CostTier::Compile),
        entries: notes::GDB_DEBUGGING,
    },
    Section {
        title: "Sidecar commands",
        cost_hint: "cost 1 — instant, LMDB-backed",
        default_cost: Some(CostTier::Instant),
        entries: tooling::SIDECAR_COMMANDS,
    },
    Section {
        title: "Comparison tools",
        cost_hint: "cost 1–5",
        default_cost: None,
        entries: tooling::COMPARISON_COMMANDS,
    },
    Section {
        title: "Cache observability (ADR 4.7.26d)",
        cost_hint: "cost 1–3",
        default_cost: None,
        entries: tooling::CACHE_COMMANDS,
    },
    Section {
        title: "Test runner",
        cost_hint: "cost 3–5",
        default_cost: Some(CostTier::CompileAndRun),
        entries: tooling::TEST_RUNNER,
    },
    Section {
        title: "Structured profiling (ADR 10.5.26j)",
        cost_hint: "cost 4",
        default_cost: Some(CostTier::Compile),
        entries: notes::PROFILING,
    },
    Section {
        title: "Cross-file diagnostic enrichment (ADR 15.5.26a)",
        cost_hint: "",
        default_cost: None,
        entries: notes::CROSS_FILE_DIAGNOSTICS,
    },
    Section {
        title: "Not diagnostics",
        cost_hint: "",
        default_cost: None,
        entries: notes::NOT_DIAGNOSTIC,
    },
];

/// Every entry across every section, flattened — the reconciliation domain.
///
/// Test-only: the renderer and the JSON emitter both need each entry's section
/// for its heading, so this flattening exists for the checks alone.
#[cfg(test)]
pub fn all_entries() -> impl Iterator<Item = &'static PipelineEntry> {
    SECTIONS.iter().flat_map(|s| s.entries.iter())
}
