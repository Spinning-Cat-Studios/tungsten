//! `tungsten doctor check codegen` — checks that run the codegen pipeline
//! (ADR 13.8.26c review).
//!
//! Grouped for the reason `cli-surface`'s own message gives: `doctor check` was
//! at 15 of 15, and these four are the one cohesive subset — each compiles the
//! program (or its codegen inputs) and reports on what emission produced,
//! rather than on source, types or an already-emitted `.ll`. Grouping them
//! takes the namespace to 13 and leaves the next author a decision instead of a
//! red gate.
//!
//! **The binary crate is not edited, and that is deliberate.** The handlers for
//! all four live in `main.rs`, so the obvious implementation adds arms there for
//! the grouped spelling. `main.rs` is the one file where that is expensive:
//! every dispatcher returns `ExitCode`, which implements neither `PartialEq` nor
//! any accessor, so no in-process test can assert on one — 9 of its 11 mutation
//! sites survive on an unmodified tree. Editing it would have added permanently
//! unkillable mutants to an enforced gate.
//!
//! [`flatten`] avoids that entirely: it rewrites the grouped spelling onto the
//! hidden flat variants *before* dispatch, so the binary sees exactly the shape
//! it always saw. The mapping is a pure function in this crate, where it is
//! unit-tested in both directions.

use std::path::PathBuf;

use clap::{Args, Subcommand};

use super::CheckCommands;

/// `doctor check codegen mono-coverage <file>`
#[derive(Args)]
pub struct MonoCoverageArgs {
    /// The root source file to check
    pub file: PathBuf,
}

/// `doctor check codegen extern-map-ambiguity <file> [--json]`
#[derive(Args)]
pub struct ExternMapAmbiguityArgs {
    /// The root source file to check
    pub file: PathBuf,

    /// Emit machine-readable JSON instead of the human report.
    #[arg(long)]
    pub json: bool,
}

/// `doctor check codegen tco-coverage <file> [flags]`
#[derive(Args)]
pub struct TcoCoverageArgs {
    /// The root source file to check (must contain a `main`).
    pub file: PathBuf,

    /// Emit machine-readable JSON instead of the table.
    #[arg(long)]
    pub json: bool,

    /// Filter to HIGH-risk rows only.
    #[arg(long = "risk", value_parser = ["high"])]
    pub risk: Option<String>,

    /// List per-call-site records instead of aggregated function rows.
    #[arg(long = "by-site")]
    pub by_site: bool,

    /// Also list EMIT/DECOMPOSE (LOW-risk) rows.
    #[arg(long)]
    pub emit: bool,

    /// Run as a deterministic CI gate (ADR 1.7.26e): exit non-zero on any
    /// un-allowlisted HIGH-risk SKIP, or any allowlist entry that participates
    /// in internal recursion. Consults `tools/tco-skip-allowlist.toml`.
    #[arg(long)]
    pub gate: bool,
}

/// `doctor check codegen unit-cost <file> [flags]`
#[derive(Args)]
pub struct UnitCostArgs {
    /// The root source file to census (must contain a `main`).
    pub file: PathBuf,

    /// Emit machine-readable JSON instead of the table.
    #[arg(long)]
    pub json: bool,

    /// Cost bound: a time like '0.5s' or an allocation volume like '8GB'.
    /// Filters the report to units meeting it AND gates the exit code
    /// (non-zero when any unit meets it).
    #[arg(long)]
    pub threshold: Option<String>,

    /// Print the comma-separated TUNGSTEN_CODEGEN_SERIAL_UNITS value for
    /// units meeting the threshold (default 0.5s). Always exits 0.
    #[arg(long = "emit-serial-list")]
    pub emit_serial_list: bool,
}

/// Codegen-pipeline health checks (ADR 13.8.26c review).
///
/// Grouped under `tungsten doctor check codegen <subcommand>`. Every variant
/// needs the `codegen` feature; the whole namespace is absent from a
/// `--no-default-features` build, which is why `commands --tree` shows a
/// different shape depending on how the binary was built.
#[derive(Subcommand)]
pub enum CheckCodegenCommands {
    /// Check mono ownership coverage for all TyApp sites (ADR 8.5.26i)
    ///
    /// Runs the mono discovery + ownership pipeline, then walks all term
    /// trees to verify every `TyApp(Global(name), ty)` has a corresponding
    /// entry in the frozen ownership map.
    ///
    /// Examples:
    ///   tungsten doctor check codegen mono-coverage src/compiler/main.tg
    #[command(name = "mono-coverage")]
    MonoCoverage(MonoCoverageArgs),

    /// Detect colliding-name extern-map misresolution (ADR 12.7.26b)
    ///
    /// The per-unit extern-name map is bare-name-keyed and clobber-last, so a
    /// unit referencing a name defined in ≥2 modules silently calls whichever
    /// candidate sorts last (ADR 12.7.26a). Reuses the exact codegen-input
    /// pipeline (no LLVM emission) and reports each ambiguous (unit, name)
    /// with all candidates and the current clobber-last pick. Exit is non-zero
    /// on any finding, so it gates CI directly.
    ///
    /// See also: `tungsten doctor check link collisions` (duplicate symbol
    /// emission — a different question: this check is call-target selection),
    /// `tungsten doctor check module name-collisions` (whether two definitions share a
    /// binding at all; parse-only, so it runs on a file this one cannot).
    ///
    /// Examples:
    ///   tungsten doctor check codegen extern-map-ambiguity src/compiler/main.tg
    ///   tungsten doctor check codegen extern-map-ambiguity main.tg --json
    #[command(name = "extern-map-ambiguity")]
    ExternMapAmbiguity(ExternMapAmbiguityArgs),

    /// Rank self-recursive functions by O(N)-stack risk from the *actual*
    /// codegen musttail decision (ADR 1.7.26b).
    #[command(long_about = super::help_text::TCO_COVERAGE)]
    #[command(name = "tco-coverage")]
    TcoCoverage(TcoCoverageArgs),

    /// Ranked per-unit codegen cost census: wall time + allocation volume
    /// (ADR 8.7.26a)
    ///
    /// Runs stage-1 codegen sequentially (jobs=1, .ll output discarded) and
    /// reports every codegen unit's wall time and allocated bytes, ranked.
    /// Replaces the ADR 3.7.26b hand-rolled census (verbose stderr → awk →
    /// manual transcription of the heavy-unit list).
    ///
    /// See also: `tungsten compile -v` (per-unit census lines during a normal
    /// build), `tungsten info type size` (stored-type-tree metrics),
    /// `TUNGSTEN_CODEGEN_SERIAL_UNITS` (the serial-unit mitigation this
    /// check's --emit-serial-list feeds).
    ///
    /// Examples:
    ///   tungsten doctor check codegen unit-cost src/compiler/main.tg --threshold 0.5s
    ///   tungsten doctor check codegen unit-cost main.tg --threshold 8GB --json
    ///   tungsten doctor check codegen unit-cost main.tg --emit-serial-list --threshold 0.5s
    #[command(name = "unit-cost")]
    UnitCost(UnitCostArgs),
}

/// Rewrite the grouped `check codegen <x>` spelling onto the hidden flat
/// variant the binary-side dispatcher matches. Anything else passes through.
///
/// Called before dispatch so `try_codegen_doctor` needs no arm for the grouped
/// form (see the module docs for why that is worth a function). Total rather
/// than fallible: a non-codegen command is not an error, it is simply not ours.
#[must_use]
pub fn flatten_codegen_check(cmd: CheckCommands) -> CheckCommands {
    match cmd {
        CheckCommands::Codegen(CheckCodegenCommands::MonoCoverage(args)) => {
            CheckCommands::MonoCoverage { file: args.file }
        }
        CheckCommands::Codegen(CheckCodegenCommands::ExternMapAmbiguity(args)) => {
            CheckCommands::ExternMapAmbiguity {
                file: args.file,
                json: args.json,
            }
        }
        CheckCommands::Codegen(CheckCodegenCommands::TcoCoverage(args)) => {
            CheckCommands::TcoCoverage {
                file: args.file,
                json: args.json,
                risk: args.risk,
                by_site: args.by_site,
                emit: args.emit,
                gate: args.gate,
            }
        }
        CheckCommands::Codegen(CheckCodegenCommands::UnitCost(args)) => CheckCommands::UnitCost {
            file: args.file,
            json: args.json,
            threshold: args.threshold,
            emit_serial_list: args.emit_serial_list,
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{flatten_codegen_check, CheckCodegenCommands, CheckCommands};
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: CheckCommands,
    }

    fn parse(args: &[&str]) -> CheckCommands {
        TestCli::try_parse_from(std::iter::once("test").chain(args.iter().copied()))
            .expect("parses")
            .cmd
    }

    /// Every grouped spelling flattens onto the flat variant the binary matches,
    /// carrying its arguments across. A field dropped here would leave the
    /// grouped form parsing and silently ignoring a flag.
    #[test]
    fn every_grouped_codegen_check_flattens_onto_its_flat_variant() {
        let CheckCommands::MonoCoverage { file } =
            flatten_codegen_check(parse(&["codegen", "mono-coverage", "a.tg"]))
        else {
            panic!("mono-coverage did not flatten");
        };
        assert_eq!(file.to_string_lossy(), "a.tg");

        let CheckCommands::ExternMapAmbiguity { file, json } = flatten_codegen_check(parse(&[
            "codegen",
            "extern-map-ambiguity",
            "b.tg",
            "--json",
        ])) else {
            panic!("extern-map-ambiguity did not flatten");
        };
        assert_eq!(file.to_string_lossy(), "b.tg");
        assert!(json, "--json survived the rewrite");

        let CheckCommands::TcoCoverage {
            file,
            json,
            risk,
            by_site,
            emit,
            gate,
        } = flatten_codegen_check(parse(&[
            "codegen",
            "tco-coverage",
            "c.tg",
            "--json",
            "--risk",
            "high",
            "--by-site",
            "--emit",
            "--gate",
        ]))
        else {
            panic!("tco-coverage did not flatten");
        };
        assert_eq!(file.to_string_lossy(), "c.tg");
        assert!(json && by_site && emit && gate, "every flag survived");
        assert_eq!(risk.as_deref(), Some("high"));

        let CheckCommands::UnitCost {
            file,
            json,
            threshold,
            emit_serial_list,
        } = flatten_codegen_check(parse(&[
            "codegen",
            "unit-cost",
            "d.tg",
            "--json",
            "--threshold",
            "8GB",
            "--emit-serial-list",
        ]))
        else {
            panic!("unit-cost did not flatten");
        };
        assert_eq!(file.to_string_lossy(), "d.tg");
        assert!(json && emit_serial_list);
        assert_eq!(threshold.as_deref(), Some("8GB"));
    }

    /// A command that is not a codegen check passes through untouched. Without
    /// this, a `flatten` that returned some fixed value would satisfy the test
    /// above and silently rewrite every other subcommand.
    #[test]
    fn a_non_codegen_check_passes_through() {
        assert!(matches!(
            flatten_codegen_check(parse(&["module", "name-collisions", "main.tg"])),
            CheckCommands::Module(_)
        ));
        assert!(matches!(
            flatten_codegen_check(parse(&["link", "health", "./bin"])),
            CheckCommands::Link(_)
        ));
    }

    /// The flat spelling is already flat, so flattening is idempotent.
    #[test]
    fn an_already_flat_spelling_is_unchanged() {
        assert!(matches!(
            flatten_codegen_check(parse(&["unit-cost", "e.tg"])),
            CheckCommands::UnitCost { .. }
        ));
        assert!(matches!(
            flatten_codegen_check(flatten_codegen_check(parse(&[
                "codegen",
                "unit-cost",
                "e.tg"
            ]))),
            CheckCommands::UnitCost { .. }
        ));
    }
}
