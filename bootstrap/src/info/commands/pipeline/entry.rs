//! The `tungsten info pipeline` inventory, as data (ADR 28.7.26f D1).
//!
//! `tungsten info pipeline` is the canonical, AI-facing inventory of the
//! compiler's diagnostic tooling. It used to be fourteen `println!`-carrying
//! functions that no assertion observed, so a subcommand could be added,
//! renamed, or removed without anything noticing. Here the inventory is a value:
//! [`PipelineEntry`] is what the reconciliation tests act on, and rendering is
//! reduced to a cosmetic pass over it.
//!
//! Two consequences worth naming:
//! - Braces in prose are ordinary characters again. The old text lived inside
//!   `println!` format strings, so writing `{ ptr, ptr }` was a compile error.
//! - Cost tiers and the "requires codegen" annotation are *derived* from
//!   [`PipelineEntry::cost`] / [`PipelineEntry::requires_codegen`] rather than
//!   hand-typed into the prose, so the reconciliation tests can check them.

/// A cost tier from the project-wide diagnostic ladder: how much work a tool
/// does before it can answer. Agents are told to start at tier 1 and escalate,
/// so a missing or wrong tier misroutes them (ADR 28.7.26f D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostTier {
    /// 1 — instant; no file I/O, or a lookup only.
    Instant,
    /// 2 — parse the input.
    Parse,
    /// 3 — parse and elaborate.
    Elaborate,
    /// 4 — full compile through codegen.
    Compile,
    /// 5 — compile and run the produced program.
    CompileAndRun,
}

impl CostTier {
    /// The tier's number on the 1–5 ladder.
    pub const fn tier(self) -> u8 {
        match self {
            Self::Instant => 1,
            Self::Parse => 2,
            Self::Elaborate => 3,
            Self::Compile => 4,
            Self::CompileAndRun => 5,
        }
    }
}

/// How an entry reconciles against the clap command tree (ADR 28.7.26f D3).
///
/// `info pipeline` legitimately documents more than subcommands — compile
/// flags, make targets, environment variables, and prose contracts — so the
/// kind decides which check applies rather than which entries get ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    /// A clap subcommand. Its `path` must resolve in the tree.
    Subcommand,
    /// A command-line flag, tagged with the clap command that accepts it —
    /// `""` for a flag global to the root command. Reconciled against that
    /// command's argument list (ADR 28.7.26f D3).
    ///
    /// The owner is part of the kind because a flag is only meaningful on the
    /// command that takes it: the listing carries `compile` flags, root-global
    /// flags, and one `check`-only flag, and reconciling all three against
    /// `compile` would be wrong in two of the three cases.
    Flag { on: &'static str },
    /// A `make` target. See the `MakeTarget` note in ADR 28.7.26f §3/§6.
    MakeTarget,
    /// An environment variable the compiler reads.
    EnvVar,
    /// Free prose — a diagram, a recipe, a shared contract. Not reconciled.
    Note,
    /// A clap leaf deliberately outside the *diagnostic* inventory, carrying the
    /// reason in-table (ADR 28.7.26f D2a). This is what makes completeness
    /// exhaustive by construction rather than an exclusion list: an
    /// unclassified leaf fails the test, and each exclusion states its `why`.
    NotDiagnostic { why: &'static str },
}

/// One documented capability.
///
/// `path` is the reconciliation key and `usage` is what a reader sees: today's
/// lines are usage forms carrying argument placeholders (`<file>`) and flag
/// variants, and two entries may describe one subcommand (`unit-cost` and
/// `unit-cost --emit-serial-list`). Keying on the display string would either
/// choke on the placeholders or key on the wrong text, so the placeholder-free
/// `path` is separate and completeness is a multimap (ADR 28.7.26f §2.1).
pub struct PipelineEntry {
    /// Canonical subcommand path — no placeholders, no flags. Empty for prose.
    pub path: &'static str,
    /// Display form, including argument placeholders and flag variants.
    pub usage: &'static str,
    /// Optional left-column label for flag tables ("Elaborate:", "Codegen:").
    pub group: &'static str,
    pub kind: EntryKind,
    /// Required for [`EntryKind::Subcommand`] — enforced by the reconciliation
    /// test, not by the type (a prose note legitimately has none).
    pub cost: Option<CostTier>,
    /// Whether the capability exists only in the codegen-featured build. Gates
    /// the no-phantom assertion, which must hold in both configurations
    /// (ADR 28.7.26f D7).
    pub requires_codegen: bool,
    /// One-line (or short multi-line) triage copy. Deliberately not derived from
    /// clap's `about`: that is help text for someone who already found the
    /// command, this is copy for someone choosing which tool to reach for
    /// (ADR 28.7.26f §2.4).
    pub summary: &'static str,
    /// Related subcommand paths, asserted to resolve in the tree.
    pub see_also: &'static [&'static str],
}

impl PipelineEntry {
    const fn bare(kind: EntryKind) -> Self {
        Self {
            path: "",
            usage: "",
            group: "",
            kind,
            cost: None,
            requires_codegen: false,
            summary: "",
            see_also: &[],
        }
    }

    /// A clap subcommand: `path` is the reconciliation key, `usage` the display
    /// form (which must begin `tungsten <path>`).
    pub const fn subcommand(
        path: &'static str,
        usage: &'static str,
        summary: &'static str,
    ) -> Self {
        Self {
            path,
            usage,
            summary,
            ..Self::bare(EntryKind::Subcommand)
        }
    }

    /// A `tungsten compile` diagnostic flag.
    pub const fn compile_flag(usage: &'static str, summary: &'static str) -> Self {
        Self::flag_on("compile", usage, summary)
    }

    /// A flag global to the root command, accepted alongside any subcommand.
    pub const fn global_flag(usage: &'static str, summary: &'static str) -> Self {
        Self::flag_on("", usage, summary)
    }

    /// A flag belonging to a specific subcommand — `on` is its clap path.
    pub const fn flag_on(on: &'static str, usage: &'static str, summary: &'static str) -> Self {
        Self {
            usage,
            summary,
            ..Self::bare(EntryKind::Flag { on })
        }
    }

    /// A `make` target that drives a diagnostic workflow.
    pub const fn make_target(usage: &'static str, summary: &'static str) -> Self {
        Self {
            usage,
            summary,
            ..Self::bare(EntryKind::MakeTarget)
        }
    }

    /// An environment variable the compiler reads.
    pub const fn env_var(usage: &'static str, summary: &'static str) -> Self {
        Self {
            usage,
            summary,
            ..Self::bare(EntryKind::EnvVar)
        }
    }

    /// Free prose rendered verbatim — a diagram, a recipe, a contract.
    pub const fn note(summary: &'static str) -> Self {
        Self {
            summary,
            ..Self::bare(EntryKind::Note)
        }
    }

    /// A clap leaf that `info pipeline` deliberately omits. The reason is both
    /// the classification and the rendered summary — the boundary the inventory
    /// draws is shown to readers, not buried in a side file.
    pub const fn not_diagnostic(path: &'static str, why: &'static str) -> Self {
        Self {
            path,
            usage: path,
            summary: why,
            ..Self::bare(EntryKind::NotDiagnostic { why })
        }
    }

    pub const fn with_cost(mut self, cost: CostTier) -> Self {
        self.cost = Some(cost);
        self
    }

    pub const fn requiring_codegen(mut self) -> Self {
        self.requires_codegen = true;
        self
    }

    pub const fn in_flag_group(mut self, group: &'static str) -> Self {
        self.group = group;
        self
    }

    pub const fn with_see_also(mut self, see_also: &'static [&'static str]) -> Self {
        self.see_also = see_also;
        self
    }
}

/// A rendered block with its own heading and cost annotation.
///
/// The section layer is load-bearing, not decoration: the output is not a flat
/// list. Sections carry headings with their own cost annotations, flag tables
/// with a left-hand stage column, and prose-only blocks (GDB recipes,
/// profiling, cross-file enrichment) — ADR 28.7.26f D1.
pub struct Section {
    /// Heading prose, or empty for the un-headed banner at the top.
    pub title: &'static str,
    /// Heading cost annotation, rendered as `[…]`. May be a range
    /// ("cost 1–5") where entries carry their own tiers.
    pub cost_hint: &'static str,
    /// The tier most entries in this section sit at. An entry whose `cost`
    /// differs is annotated individually, which is what keeps the rendered
    /// `[cost N]` markers derived rather than hand-typed.
    pub default_cost: Option<CostTier>,
    pub entries: &'static [PipelineEntry],
}
