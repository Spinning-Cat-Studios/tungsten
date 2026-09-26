//! `tungsten info eval` — evaluator inspection subcommands (ADR 21.7.26j).
//!
//! A new `info` noun for "show me X while evaluating", which is neither a
//! `type`, `codegen`, `module`, nor `cir` question. Three subcommands: `trace`
//! (what the evaluator does with a definition), `externs` (what the evaluator
//! can execute at all) and `reachable-externs` (which of those a *particular*
//! definition's call path reaches).

pub mod externs;
pub mod reachable_externs;
pub mod trace;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

/// Evaluator inspection subcommands, accessed via `tungsten info eval <sub>`.
#[derive(Subcommand)]
pub enum InfoEvalCommands {
    /// Trace the evaluator step-by-step for a definition (cost 3, no codegen)
    ///
    /// Elaborates the project, then drives the small-step evaluator on the
    /// named definition's body, printing per step: the step index, a bounded
    /// node count, and a depth-limited shape rendering. Answers "is it looping,
    /// growing, or stuck?" — the 21.7.26e wall-2 probe, productized.
    ///
    /// Examples:
    ///   tungsten info eval trace main examples/hello.tg
    ///   tungsten info eval trace `test_compose` src/compiler/main.tg --max-steps 50
    #[command(
        after_help = "See also: `tungsten test <file>` runs the same evaluator under a watchdog."
    )]
    Trace {
        /// Definition name to trace (e.g., "main", "`test_compose`")
        name: String,

        /// The source file (or project entry file) containing the definition
        file: PathBuf,

        /// Stop after this many reduction steps
        #[arg(long, default_value_t = 1000)]
        max_steps: usize,

        /// Levels of structure to render per step
        #[arg(long, default_value_t = 2)]
        shape_depth: usize,

        /// Node-counting budget per step (a larger term reports `≥M`)
        #[arg(long, default_value_t = 100_000)]
        limit_nodes: usize,
    },

    /// List the `tg_*` externs the evaluator can execute (cost 1)
    ///
    /// The evaluator executes only allowlisted externs; every other
    /// `ExternCall` goes **silently Stuck** — no error, no output, the call
    /// simply never happens. This is the allowlist. Reads no file.
    ///
    /// Examples:
    ///   tungsten info eval externs
    ///   tungsten info eval externs --json
    #[command(
        after_help = "See also: `tungsten doctor check extern-coverage <file>` asks the same \nquestion of the externs one file declares, and `tungsten info eval \nreachable-externs <def> <file>` asks it of one definition's call path."
    )]
    Externs {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Which externs one or more definitions' call paths reach (cost 3, no codegen)
    ///
    /// Walks the Core call graph from `<def>`, reporting every extern it
    /// reaches, whether the evaluator can execute it, and the shortest call
    /// chain that gets there. Answers "can I unit-test this at cost 5, or will
    /// the assertion go silently Stuck?" — which neither sibling can:
    /// `info eval externs` knows the allowlist but not your code, and
    /// `doctor check extern-coverage` reports **declaration** scope, so it
    /// reads identically for a sound file and a vacuous one.
    ///
    /// When nothing blocks, it also answers the INVERSE — assertable at cost 5,
    /// yet no `test_*` in this entry file calls it, so coverage it could carry
    /// today is going unused. That is per entry file, like
    /// `doctor audit-dead-definitions`, and counts DIRECT calls only: a broad
    /// suite transitively reaches most of the driver, which would make the flag
    /// fire nowhere (ADR 19.8.26c review).
    ///
    /// `--defs` adds roots: every definition named is answered from ONE
    /// elaboration, which is ~99% of the invocation's cost. `--max-visited`
    /// bounds each walk; hitting it reports `INCOMPLETE` and names what went
    /// unread, rather than a shorter list that reads like a complete one.
    ///
    /// Examples:
    ///   tungsten info eval reachable-externs `classify_test_signature` src/compiler/main.tg
    ///   tungsten info eval reachable-externs `harness_artifact_path_of` src/compiler/main.tg --json
    ///   tungsten info eval reachable-externs `parse_args_list` src/compiler/main.tg --defs `parse_info_command,parse_check_command`
    #[command(
        after_help = "See also: `tungsten info eval externs` lists the allowlist itself; \n`tungsten test <file> --assertion-census` shows, after the fact, which \nassertions actually ran.\n\n--defs answers several definitions from one elaboration, one section each \nin the order requested; --json then emits an outer array whose elements \nare exactly what the single-definition form emits. --max-visited caps the \ndefinitions each walk enters: the report says INCOMPLETE and names what it \ndid not reach, and --json carries a `complete` field so a gate reading \n`blocking` cannot mistake a partial zero for a real one."
    )]
    ReachableExterns {
        /// Definition name to walk from (e.g., "`classify_test_signature`")
        name: String,

        /// The source file (or project entry file) containing the definition
        file: PathBuf,

        /// Additional definitions to walk from the same elaboration
        /// (comma-separated, or repeat the flag)
        #[arg(long, value_delimiter = ',')]
        defs: Vec<String>,

        /// Definitions each walk may enter before it reports a partial answer
        #[arg(long, default_value_t = reachable_externs::DEFAULT_MAX_VISITED)]
        max_visited: usize,

        /// Output as JSON, including a `blocking` count for use in a gate
        #[arg(long)]
        json: bool,
    },
}

/// Dispatch evaluator-related info subcommands.
pub fn dispatch_eval_info(cmd: InfoEvalCommands, verbose: bool, max_errors: usize) -> ExitCode {
    match cmd {
        InfoEvalCommands::Trace {
            name,
            file,
            max_steps,
            shape_depth,
            limit_nodes,
        } => {
            let opts = trace::TraceOptions {
                max_steps,
                shape_depth,
                limit_nodes,
            };
            trace::cmd_info_eval_trace(&name, &file, verbose, max_errors, &opts)
        }
        InfoEvalCommands::Externs { json } => externs::cmd_info_eval_externs(json),
        InfoEvalCommands::ReachableExterns {
            name,
            file,
            defs,
            max_visited,
            json,
        } => {
            let opts = reachable_externs::ReachableExternsOptions {
                name,
                defs,
                file,
                max_visited,
                json,
            };
            reachable_externs::cmd_info_eval_reachable_externs(&opts, verbose, max_errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `dispatch_eval_info` routes a `Trace` command through to the tracer.
    #[test]
    fn dispatch_trace_routes_to_the_trace_command() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("m.tg");
        std::fs::write(&path, "fn main() -> Nat { 5 }\n").unwrap();
        let cmd = InfoEvalCommands::Trace {
            name: "main".to_string(),
            file: path,
            max_steps: 100,
            shape_depth: 2,
            limit_nodes: 1000,
        };
        assert_eq!(dispatch_eval_info(cmd, false, 20), ExitCode::SUCCESS);
    }

    /// A `Trace` for an absent definition routes through and reports failure —
    /// so `dispatch_eval_info` is not a constant-success shim.
    #[test]
    fn dispatch_trace_propagates_failure_for_missing_def() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("m.tg");
        std::fs::write(&path, "fn main() -> Nat { 0 }\n").unwrap();
        let cmd = InfoEvalCommands::Trace {
            name: "absent".to_string(),
            file: path,
            max_steps: 100,
            shape_depth: 2,
            limit_nodes: 1000,
        };
        assert_eq!(dispatch_eval_info(cmd, false, 20), ExitCode::FAILURE);
    }

    /// `dispatch_eval_info` routes an `Externs` command through to the registry
    /// renderer, in both output modes — a mis-wired arm would leave the
    /// subcommand reachable from the CLI but doing nothing.
    #[test]
    fn dispatch_externs_routes_to_the_externs_command() {
        assert_eq!(
            dispatch_eval_info(InfoEvalCommands::Externs { json: false }, false, 20),
            ExitCode::SUCCESS
        );
        assert_eq!(
            dispatch_eval_info(InfoEvalCommands::Externs { json: true }, false, 20),
            ExitCode::SUCCESS
        );
    }

    /// `dispatch_eval_info` routes a `ReachableExterns` command through to the
    /// walker, in both output modes.
    #[test]
    fn dispatch_reachable_externs_routes_to_the_walker() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("m.tg");
        std::fs::write(&path, "fn main() -> Nat { 7 }\n").unwrap();
        for json in [false, true] {
            let cmd = InfoEvalCommands::ReachableExterns {
                name: "main".to_string(),
                file: path.clone(),
                defs: Vec::new(),
                max_visited: reachable_externs::DEFAULT_MAX_VISITED,
                json,
            };
            assert_eq!(dispatch_eval_info(cmd, false, 20), ExitCode::SUCCESS);
        }
    }

    /// The set form routes through with its extra roots, in both output modes —
    /// a dropped `defs` field would leave `--defs` accepted by clap and inert.
    #[test]
    fn dispatch_reachable_externs_routes_the_set_form_and_the_budget() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("m.tg");
        std::fs::write(
            &path,
            "fn helper() -> Nat { 1 }\nfn main() -> Nat { helper() }\n",
        )
        .unwrap();
        for json in [false, true] {
            let cmd = InfoEvalCommands::ReachableExterns {
                name: "main".to_string(),
                file: path.clone(),
                defs: vec!["helper".to_string()],
                max_visited: 1,
                json,
            };
            assert_eq!(dispatch_eval_info(cmd, false, 20), ExitCode::SUCCESS);
        }
    }

    /// One bad name in a set still fails, so a misspelling cannot pass as a
    /// clean answer about the roots that did resolve.
    #[test]
    fn dispatch_reachable_externs_fails_when_one_of_several_roots_is_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("m.tg");
        std::fs::write(&path, "fn main() -> Nat { 0 }\n").unwrap();
        let cmd = InfoEvalCommands::ReachableExterns {
            name: "main".to_string(),
            file: path,
            defs: vec!["absent".to_string()],
            max_visited: reachable_externs::DEFAULT_MAX_VISITED,
            json: false,
        };
        assert_eq!(dispatch_eval_info(cmd, false, 20), ExitCode::FAILURE);
    }

    /// An absent definition reports failure rather than an empty clean report —
    /// the difference between "walked it, found nothing" and "never found it".
    #[test]
    fn dispatch_reachable_externs_propagates_failure_for_missing_def() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("m.tg");
        std::fs::write(&path, "fn main() -> Nat { 0 }\n").unwrap();
        let cmd = InfoEvalCommands::ReachableExterns {
            name: "absent".to_string(),
            file: path,
            defs: Vec::new(),
            max_visited: reachable_externs::DEFAULT_MAX_VISITED,
            json: false,
        };
        assert_eq!(dispatch_eval_info(cmd, false, 20), ExitCode::FAILURE);
    }
}
