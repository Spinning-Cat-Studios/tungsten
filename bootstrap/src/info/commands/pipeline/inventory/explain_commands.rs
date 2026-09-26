//! `tungsten explain`, for the `info pipeline` inventory.
//!
//! Split from [`super::doctor_commands`], which carried it only for historical
//! reasons: `explain` is a static explainer with no file I/O, and shares
//! nothing with the health checks but a source file.

use super::{CostTier, PipelineEntry};

pub const EXPLAIN_COMMANDS: &[PipelineEntry] = &[
    PipelineEntry::subcommand(
        "explain error",
        "tungsten explain error [<code|kind>]",
        "Explain a bootstrap elaboration error, by the CODE the compiler\n\
         printed (E0010) or the kind name (TypeMismatch), case-insensitively\n\
         (ADR 8.8.26a). No argument lists every user-facing code (E9998, the\n\
         internal-invariant code, resolves but is unlisted). --self-hosted [<code>]\n\
         explains a self-hosted-compiler code instead — those are numbered\n\
         differently and are NOT interchangeable",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "explain type",
        "tungsten explain type <string>",
        "Decode a structural Core IR type",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "explain recursion-types",
        "tungsten explain recursion-types",
        "Classification of recursion patterns",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "explain stack-overflow",
        "tungsten explain stack-overflow",
        "Understanding stack overflow crashes",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "explain mutual-recursion",
        "tungsten explain mutual-recursion",
        "Understanding mutual type recursion",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "explain keywords",
        "tungsten explain keywords [<name>]",
        "List the reserved words, or ask whether ONE name is available. An\n\
         identifier equal to a keyword fails to PARSE, and the error is\n\
         reported at the token AFTER it — so `fn f(sym: T)` underlines `T`.\n\
         The proof keywords are the trap: by, have, show, sym, cong, trans.\n\
         The set is generated from the lexer's own table (ADR 7.8.26b\n\
         retrospective), so it cannot drift from what the compiler enforces",
    )
    .with_cost(CostTier::Instant),
];
