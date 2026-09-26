//! `tungsten doctor check ir` — IR-related health checks (ADR 12.5.26h).
//!
//! Groups IR validation checks under `doctor check ir ...`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

use crate::doctor::checks;

/// IR-related health check subcommands (ADR 12.5.26h).
///
/// Grouped under `tungsten doctor check ir <subcommand>`.
#[derive(Subcommand)]
pub enum CheckIrCommands {
    /// Check store/load type-width consistency in LLVM IR (ADR 21.4.26b)
    ///
    /// Parses an emitted .ll file and reports store/load instructions
    /// where the value type width disagrees with the pointer target type.
    ///
    /// Examples:
    ///   tungsten doctor check ir layout output.ll
    Layout {
        /// The LLVM IR (.ll) file to check
        file: PathBuf,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },

    /// Validate per-unit declaration hygiene in emitted LLVM IR (ADR 10.5.26b)
    ///
    /// Scans the `define … { … }` bodies of `.ll` files in a directory and
    /// reports any direct `call @symbol` target that lacks a matching `declare`
    /// or `define` in the same file. Module-scope globals are skipped: the
    /// self-hosted compiler emits LLVM IR *as string literals*, and scanning
    /// them produced five false findings (ADR 28.7.26e).
    ///
    /// Examples:
    ///   tungsten doctor check ir declares target/ll/
    ///   tungsten doctor check ir declares target/ll/ --strict
    ///
    /// See also: `tungsten compile --emit-llvm`, `tungsten info codegen units`
    Declares {
        /// Directory containing `.ll` files to scan
        dir: Option<PathBuf>,

        /// Deprecated alias for the positional <DIR>. This audit took a flag
        /// where its five siblings take a positional, which is part of why ADR
        /// 28.7.26e's own first pass failed to enumerate it (§1.2). Kept working
        /// so existing scripts and the `doctor check declares` legacy alias do
        /// not break.
        #[arg(long, hide = true)]
        from_existing_ir: Option<PathBuf>,

        /// Fail (exit 2) on a vacuous pass: call targets seen but none resolved
        #[arg(long)]
        strict: bool,
    },
    /// Scan for null function pointer calls in emitted LLVM IR (ADR 10.5.26d)
    ///
    /// Searches `.ll` files for a call whose **callee position** is the `null`
    /// literal, indicating unresolved monomorphization — a mono instance was
    /// expected but the function pointer was never filled in. A named callee is
    /// never one, however its name ends: the pre-28.7.26e substring heuristic
    /// reported 15 false findings on `@cstring_is_null(`.
    ///
    /// Examples:
    ///   tungsten doctor check ir null-calls target/ll/
    ///   tungsten doctor check ir null-calls target/ll/ --strict
    NullCalls {
        /// Directory containing `.ll` files to scan
        dir: PathBuf,

        /// Fail (exit 2) on a vacuous pass: calls seen but no callee resolved
        #[arg(long)]
        strict: bool,
    },

    /// Audit Class-P indirect-param buffer discipline in LLVM IR — the standing
    /// `noalias` oracle (ADRs 1.7.26e, 17.7.26e).
    ///
    /// Callee arm: for each `$direct_mt` with a self-`musttail`, verifies its
    /// buffer/sret/env pointers are *forwarded* (never a fresh per-iteration
    /// `alloca`, R10), that the buffer slots of the tail edge are pairwise
    /// distinct (I5 — one pointer in two slots would alias two `noalias` params
    /// on the next activation), and that a buffer pointer never escapes
    /// (stored-as-value or returned, R4).
    /// Shim arm: for each `$direct` shim calling a `$direct_mt` with buffer
    /// slots, verifies its `sret_buf`/`indirect_buf.*` allocas are used only for
    /// {alloca, address derivation, lifetime/debug/memory intrinsics, the fill
    /// store, the `$direct_mt` argument, the sret read-back} (I4).
    /// Run `tungsten compile --emit-llvm` first, then point this at the output dir.
    ///
    /// See also: `tungsten info codegen indirect-abi`, `tungsten doctor check
    /// tco-coverage`.
    ///
    /// Prints a machine-readable `summary:` line with per-arm candidate/tracked
    /// counts. With `--strict` (CI), a vacuous pass — an arm's candidate
    /// functions exist but zero buffers were tracked (parser/format drift,
    /// ADR 2.7.26b T5a; a `shim.rs` buffer rename, ADR 17.7.26e) — is a failure.
    ///
    /// Examples:
    ///   tungsten doctor check ir indirect-buffers target/ll/
    ///   tungsten doctor check ir indirect-buffers target/ll/ --strict
    #[command(name = "indirect-buffers")]
    IndirectBuffers {
        /// Directory containing `.ll` files to scan
        dir: PathBuf,

        /// Fail (exit ≠ 0) on a vacuous pass: candidates > 0 but zero buffers tracked
        #[arg(long)]
        strict: bool,
    },

    /// Lint sret-return shapes in emitted LLVM IR (ADR 3.7.26d).
    ///
    /// Canonical-shape lint, not a general sret ABI verifier: in every
    /// function with an `sret(T)` parameter, each `ret void` must be
    /// immediately preceded by a store through the sret parameter or a
    /// `musttail call` forwarding it. A bare `call …; ret void` — the ADR
    /// 3.7.26a defect-2 silent-miscompile shape (result computed, never
    /// stored to the out-pointer) — is reported as a suspicious bare-return
    /// shape. Headers that mention `sret(` but cannot be parsed fail closed.
    /// Run `tungsten compile --emit-llvm` first, then point this at the
    /// output dir.
    ///
    /// See also: `tungsten diff exec` (dynamic evaluator-vs-native parity),
    /// `tungsten doctor check ir indirect-buffers`.
    ///
    /// Examples:
    ///   tungsten doctor check ir sret-stores target/ll/
    ///   tungsten doctor check ir sret-stores target/ll/ --strict
    #[command(name = "sret-stores")]
    SretStores {
        /// Directory containing `.ll` files to scan
        dir: PathBuf,

        /// Fail (exit 2) on a vacuous pass: sret functions found but no
        /// `ret void` site classified
        #[arg(long)]
        strict: bool,
    },

    /// Flag truncating merge memcpys in emitted LLVM IR (ADR 2.7.26b T6).
    ///
    /// Scans `.ll` files for a `memcpy` whose copy size is SMALLER than the
    /// aggregate alloca it reads/writes, with the loaded result feeding a
    /// `phi` — the 1.7.26e §6.6 phi-poisoning signature (a merge typed from a
    /// dead musttail arm's dummy, truncating real sret results through a
    /// 1-byte memcpy). Heuristic text backstop behind the `cast_to_type`
    /// shrinking-aggregate hard error; only direct-alloca memcpy operands are
    /// matched (documented reach). Exit codes: 0 clean, 1 matches, 2 bad input.
    /// Run `tungsten compile --emit-llvm` first, then point this at the output
    /// dir.
    ///
    /// See also: `tungsten doctor check ir indirect-buffers`, `tungsten doctor
    /// check tco-coverage`.
    ///
    /// Examples:
    ///   tungsten doctor check ir merge-truncation target/ll/
    ///   tungsten doctor check ir merge-truncation target/ll/ --strict
    #[command(name = "merge-truncation")]
    MergeTruncation {
        /// Directory containing `.ll` files to scan
        dir: PathBuf,

        /// Fail (exit 2) on a vacuous pass: functions scanned but no
        /// alloca/load/phi fact bound
        #[arg(long)]
        strict: bool,
    },

    /// Flag depot-instance wrapper self-calls in emitted LLVM IR (ADR 23.7.26c).
    ///
    /// A monomorphized generic instance's saturated self-call must ride its
    /// `$direct` entry; if its own `$direct`/`$direct_mt`/wrapper body `call`s
    /// its closure-returning wrapper `@<W>`, each recursion step heap-allocates
    /// an environment (the self-compile-scale OOM this ADR fixed). Precise: a
    /// legitimate higher-order call to a *different* instance's wrapper is not
    /// flagged, and neither is an arity-1 instance's saturated self-call, whose
    /// return type is its result aggregate rather than a closure pair
    /// (ADR 28.7.26e §2.3). Run `tungsten compile --emit-llvm` first, then
    /// point this at the output dir.
    ///
    /// See also: `tungsten doctor check ir null-calls`, `tungsten doctor check
    /// tco-coverage`.
    ///
    /// Examples:
    ///   tungsten doctor check ir wrapper-self-calls target/ll/
    ///   tungsten doctor check ir wrapper-self-calls target/ll/ --strict
    #[command(name = "wrapper-self-calls")]
    WrapperSelfCalls {
        /// Directory containing `.ll` files to scan
        dir: PathBuf,

        /// Fail (exit 2) on a vacuous pass: instance bodies entered but no
        /// call line examined
        #[arg(long)]
        strict: bool,
    },
}

/// Dispatch an IR-related health check subcommand.
pub fn dispatch_check_ir(cmd: CheckIrCommands) -> ExitCode {
    match cmd {
        CheckIrCommands::Layout { file, json } => {
            checks::check_ir_layout::cmd_check_ir_layout(&file, json)
        }
        CheckIrCommands::Declares {
            dir,
            from_existing_ir,
            strict,
        } => declares_corpus_dir(dir, from_existing_ir).map_or_else(missing_declares_dir, |dir| {
            checks::check_declares::cmd_check_declares(&dir, strict)
        }),
        CheckIrCommands::NullCalls { dir, strict } => {
            checks::check_null_calls::cmd_check_null_calls(&dir, strict)
        }
        CheckIrCommands::IndirectBuffers { dir, strict } => {
            checks::check_indirect_buffers::cmd_check_indirect_buffers(&dir, strict)
        }
        CheckIrCommands::SretStores { dir, strict } => {
            checks::check_sret_stores::cmd_check_sret_stores(&dir, strict)
        }
        CheckIrCommands::MergeTruncation { dir, strict } => {
            checks::check_merge_truncation::cmd_check_merge_truncation(&dir, strict)
        }
        CheckIrCommands::WrapperSelfCalls { dir, strict } => {
            checks::check_wrapper_self_calls::cmd_check_wrapper_self_calls(&dir, strict)
        }
    }
}

/// The corpus directory for `declares`: the positional `<DIR>`, else its
/// deprecated `--from-existing-ir` alias.
///
/// A value rather than a nested `match` in the dispatcher, for two reasons: the
/// dispatcher is a flat one-`match` arm table and a second level trips the
/// match-depth gate, and the precedence rule ("positional wins") is a decision
/// worth asserting rather than reading.
fn declares_corpus_dir(
    positional: Option<PathBuf>,
    deprecated_flag: Option<PathBuf>,
) -> Option<PathBuf> {
    positional.or(deprecated_flag)
}

/// Report the missing corpus directory. Exit 2 = bad input, matching the audits'
/// own contract (0 clean / 1 findings / 2 bad input).
fn missing_declares_dir() -> ExitCode {
    eprintln!(
        "error: missing <DIR> — the directory of `.ll` files to scan\n\
         usage: tungsten doctor check ir declares <DIR> [--strict]"
    );
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `declares` accepts a positional `<DIR>` like its five siblings, and still
    /// honours the deprecated `--from-existing-ir` flag it shipped with — the
    /// odd-one-out spelling that helped ADR 28.7.26e's first pass overlook the
    /// audit entirely (§1.2).
    #[test]
    fn the_positional_dir_wins_over_the_deprecated_flag() {
        let positional = PathBuf::from("positional/ll");
        let flag = PathBuf::from("flag/ll");

        assert_eq!(
            declares_corpus_dir(Some(positional.clone()), None),
            Some(positional.clone()),
            "the positional form is the documented one"
        );
        assert_eq!(
            declares_corpus_dir(None, Some(flag.clone())),
            Some(flag.clone()),
            "the deprecated flag still works, so scripts do not break"
        );
        assert_eq!(
            declares_corpus_dir(Some(positional.clone()), Some(flag)),
            Some(positional),
            "given both, the positional wins — one rule, not an error"
        );
        assert_eq!(
            declares_corpus_dir(None, None),
            None,
            "neither is the caller's mistake, reported as bad input"
        );
    }

    /// Missing input is bad input (exit 2), not a clean run and not a finding.
    #[test]
    fn a_missing_corpus_dir_exits_as_bad_input() {
        let rendered = format!("{:?}", missing_declares_dir());
        assert_eq!(rendered, format!("{:?}", ExitCode::from(2)));
        assert_ne!(rendered, format!("{:?}", ExitCode::SUCCESS));
        assert_ne!(
            rendered,
            format!("{:?}", ExitCode::FAILURE),
            "bad input is distinct from a finding"
        );
    }
}
