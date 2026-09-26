//! `tungsten test` — test discovery, execution, and reporting (ADR 5.5.26a).

mod discovery;
mod run;
mod scope;
mod summary;
mod tier;

#[cfg(test)]
mod tests;

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::cli::ColorMode;

use discovery::{classify_empty_suite, discover_tests, EmptySuiteAction};
use scope::{scope_defs_to_module, ModuleScopeResult};

/// ANSI text styles for test output.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Style {
    Green,
    Red,
    Yellow,
    Bold,
    BoldGreen,
    BoldRed,
}

/// Apply an ANSI style to a string. Returns the string unchanged when `use_color` is false.
fn paint(s: &str, style: Style, use_color: bool) -> String {
    if !use_color {
        return s.to_string();
    }
    match style {
        Style::Green => format!("\x1b[32m{s}\x1b[0m"),
        Style::Red => format!("\x1b[31m{s}\x1b[0m"),
        Style::Yellow => format!("\x1b[33m{s}\x1b[0m"),
        Style::Bold => format!("\x1b[1m{s}\x1b[0m"),
        Style::BoldGreen => format!("\x1b[1;32m{s}\x1b[0m"),
        Style::BoldRed => format!("\x1b[1;31m{s}\x1b[0m"),
    }
}

use tungsten_bootstrap::driver::{self, Mode, PipelineOpts, PipelineResult};

/// A discovered test function.
#[derive(Debug)]
struct TestFunction {
    name: String,
}

/// Result of running a single test.
#[derive(Debug)]
enum TestOutcome {
    Passed,
    Failed(String),
    Skipped(String),
    /// The watchdog deadline elapsed before the body reached a value
    /// (ADR 21.7.26f / D1). Counted as a failure, but reported distinctly:
    /// "this test never came back" is a different diagnosis from
    /// "this test's assertion was false".
    TimedOut {
        secs: u64,
        steps: u64,
    },
    /// A global re-entered its own forcing during evaluation — it has no
    /// value (ADR 22.7.26a). Counted as a failure, reported distinctly from
    /// both `Failed` and `TimedOut`: pre-detection this shape was a stack
    /// overflow, and folding it into a passing "stuck" outcome would let the
    /// test go silently green (§1.3). The cycle is the ordered path,
    /// closed — `["f", "f"]` for direct self-reference.
    BlackHole {
        cycle: Vec<String>,
    },
    /// A structural comparison never ran (ADR 1.8.26b D3): `compare<T>` at a
    /// `T` whose comparator could not be synthesized, a residual comparison
    /// reaching an assertion, or a projection into a non-pair. Counted as a
    /// failure and reported distinctly for the same reason `BlackHole` is —
    /// pre-detection every one of these was reported **`ok`**, because the
    /// assertion never executed and so never set the failure flag. "This test
    /// asserted nothing" is a different diagnosis from "this assertion was
    /// false", and conflating them is what made the defect class invisible.
    NeverCompared {
        reason: String,
    },
    /// The body reached a value having executed **zero** assertions
    /// (ADR 6.8.26b). Counted as a failure and reported distinctly for the
    /// same reason `NeverCompared` is, and it covers the route that one
    /// cannot: 1.8.26b's guard inspects an assertion's operands, so it fires
    /// only when the assertion runs. When an enclosing expression goes `Stuck`
    /// first — an extern outside `EXECUTABLE_EXTERNS`, say — the assertion is
    /// never reached, there are no operands to inspect, and the test used to
    /// report `ok`. Counting executions catches both without knowing why.
    AssertedNothing,
    /// The body executed at least one assertion and then stopped making
    /// progress (ADR 6.8.26b D7) — it reached a residual, not `Unit`.
    ///
    /// The evaluator returns a stuck term as a *value* (`StepResult::Stuck =>
    /// return current`), so this arrives as `Ok`, not `Err`, and the runner
    /// used to discard it. A test whose *third* assertion sticks therefore
    /// reported `ok` on a count of two — partial vacuity that the
    /// zero-assertion check cannot see, because the count is nonzero.
    ///
    /// Ordered AFTER `AssertedNothing`: a body that sticks before its first
    /// assertion has both a zero count and a residual, and "asserted nothing"
    /// is the more useful of the two diagnoses.
    DidNotFinish {
        assertions: u64,
    },
    /// A test the manifest lists as expected to fail, which duly failed
    /// (ADR 6.8.26c D6). Reported and named, but not gating.
    ///
    /// `owner` is the successor ADR that will remove the entry — required by
    /// the manifest schema, because an expected failure with nobody's name on
    /// it is just a disabled test. `reported` carries the outcome it would
    /// otherwise have had, so the reader still sees *how* it failed rather
    /// than only that it was permitted to.
    ExpectedFailure {
        owner: String,
        reported: Box<TestOutcome>,
    },
}

impl TestOutcome {
    /// The short label this outcome prints under, used for the nested line an
    /// `ExpectedFailure` reports its underlying diagnosis on.
    fn label(&self) -> &'static str {
        match self {
            Self::Passed => "ok",
            Self::Failed(_) => "FAILED",
            Self::Skipped(_) => "skipped",
            Self::TimedOut { .. } => "TIMEOUT",
            Self::BlackHole { .. } => "BLACK HOLE",
            Self::NeverCompared { .. } => "NEVER COMPARED",
            Self::AssertedNothing => "ASSERTED NOTHING",
            Self::DidNotFinish { .. } => "DID NOT FINISH",
            Self::ExpectedFailure { .. } => "EXPECTED FAILURE",
        }
    }
}

/// Options for the test command.
pub struct TestOptions<'a> {
    pub file: &'a PathBuf,
    pub filter: Option<&'a str>,
    pub module: Option<&'a str>,
    pub check_only: bool,
    pub require_tests: bool,
    /// Print the per-test executed-assertion count (ADR 6.8.26b).
    pub assertion_census: bool,
    /// Per-test wall-clock bound in seconds; `0` disables the watchdog
    /// (ADR 21.7.26f / D1).
    pub watchdog_secs: u64,
    pub color: ColorMode,
    pub verbose: bool,
    pub max_errors: usize,
    pub dump_types: bool,
}

/// What `tg-test-tiers.toml` says about this entry file.
struct ManifestVerdict {
    tier: Option<tier::CostTier>,
    /// Test name → the successor ADR owning its expected-failure entry.
    expected_failures: std::collections::BTreeMap<String, String>,
}

/// Resolve `file`'s manifest verdict, or the empty one if nothing governs it.
///
/// Resolved BEFORE the pipeline runs, so a mis-declaration fails in
/// milliseconds rather than after a whole-compiler elaboration.
fn manifest_verdict(file: &Path) -> Result<ManifestVerdict, tier::TierError> {
    let Some(manifest) = tier::TierManifest::governing(file)? else {
        return Ok(ManifestVerdict {
            tier: None,
            expected_failures: std::collections::BTreeMap::new(),
        });
    };
    let source = std::fs::read_to_string(file).unwrap_or_default();
    Ok(ManifestVerdict {
        tier: manifest.tier_for(file, &source)?,
        expected_failures: manifest.expected_failures(file),
    })
}

/// Run the test command.
pub fn cmd_test(opts: &TestOptions<'_>) -> ExitCode {
    let verdict = match manifest_verdict(opts.file) {
        Ok(verdict) => verdict,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let pipeline_opts = PipelineOpts {
        mode: Mode::Test,
        verbose: opts.verbose,
        dump_types: opts.dump_types,
    };

    let (defs, module_defs, comparator_types) =
        match driver::run_file_with_options(opts.file, &pipeline_opts, false, opts.max_errors) {
            Ok(PipelineResult::Tested {
                defs,
                module_defs,
                comparator_types,
                ..
            }) => (defs, module_defs, comparator_types),
            Ok(PipelineResult::Failed) => return ExitCode::FAILURE,
            Ok(_) => {
                eprintln!("error: unexpected pipeline result");
                return ExitCode::FAILURE;
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(3);
            }
        };

    // Scope defs to target module if --module is provided (ADR 12.5.26b).
    // `defs` (the full set) is retained as the evaluator's globals so a scoped test
    // can still call helpers defined in other modules (ADR 29.6.26f / T13).
    let scoped_defs = if let Some(module_target) = opts.module {
        let project_root = opts.file.parent().unwrap_or(Path::new("."));
        match scope_defs_to_module(&module_defs, module_target, project_root) {
            ModuleScopeResult::Matched(defs) => defs,
            ModuleScopeResult::NoMatch => {
                if opts.require_tests {
                    eprintln!(
                        "error: no module matching '{module_target}' found and --require-tests is set"
                    );
                    return ExitCode::FAILURE;
                }
                eprintln!("warning: no module matching '{module_target}' found; 0 tests run");
                return ExitCode::SUCCESS;
            }
            ModuleScopeResult::Ambiguous(paths) => {
                eprintln!("error: module path '{module_target}' is ambiguous, matched:");
                for p in &paths {
                    eprintln!("  {}", p.display());
                }
                return ExitCode::FAILURE;
            }
        }
    } else {
        defs.clone()
    };

    let (tests, discovery_errors) = discover_tests(&scoped_defs, opts.filter);

    // Report discovery errors
    for err in &discovery_errors {
        eprintln!("warning: skipping {}: {}", err.name, err.reason);
    }

    match classify_empty_suite(tests.len(), discovery_errors.len(), opts.require_tests) {
        EmptySuiteAction::FailRequireTests => {
            eprintln!(
                "error: zero runnable tests discovered (--require-tests); {} function(s) skipped",
                discovery_errors.len()
            );
            return ExitCode::FAILURE;
        }
        EmptySuiteAction::ReportNoTests => {
            println!("no tests found");
            return ExitCode::SUCCESS;
        }
        EmptySuiteAction::Proceed => {}
    }

    let use_color = match opts.color {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => std::io::stdout().is_terminal(),
    };

    let run_opts = run::RunOptions {
        check_only: tier::should_skip_bodies(opts.check_only, verdict.tier),
        watchdog_secs: opts.watchdog_secs,
        use_color,
        assertion_census: opts.assertion_census,
        expected_failures: verdict.expected_failures,
    };
    run::run_and_report(&tests, &defs, &comparator_types, &run_opts)
}
