//! The `tungsten doctor check ir` family, for the `info pipeline` inventory.
//!
//! The pilot port of ADR 28.7.26f: this is the block ADR 28.7.26e last churned,
//! and the one whose `replace print_doctor_ir_checks with ()` mutant sat in
//! `tools/mutant-survivor-allowlist.toml` because the renderer was print-only
//! and unasserted. As data the entries are killable, and the allowlist entry
//! goes away with the function.

use super::{CostTier, PipelineEntry};

pub const DOCTOR_IR_AUDITS: &[PipelineEntry] = &[
    PipelineEntry::subcommand(
        "doctor check ir layout",
        "tungsten doctor check ir layout <file.ll>",
        "Check store/load type-width consistency",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "doctor check ir declares",
        "tungsten doctor check ir declares <dir> [--strict]",
        "Validate call/declare hygiene in .ll function BODIES — module-scope\n\
         globals are data, not calls (the self-hosted compiler emits IR as\n\
         string literals; ADR 28.7.26e)",
    )
    .with_cost(CostTier::Parse),
    PipelineEntry::subcommand(
        "doctor check ir null-calls",
        "tungsten doctor check ir null-calls <dir> [--strict]",
        "Flag a call whose CALLEE POSITION is the null literal — unresolved\n\
         monomorphization. A named callee is never one, however its name ends\n\
         (`@cstring_is_null(`; ADRs 10.5.26d, 28.7.26e D2)",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "doctor check ir indirect-buffers",
        "tungsten doctor check ir indirect-buffers <dir> [--strict]",
        "Audit Class-P indirect-param buffer discipline — the noalias oracle:\n\
         callee arm (no forwarded allocas / escapes / aliased tail-edge slots,\n\
         ADRs 1.7.26e + 17.7.26e I5) + shim arm (buffer used only in the I4\n\
         allowlist)",
    )
    .with_cost(CostTier::Instant)
    .with_see_also(&["info codegen indirect-abi"]),
    PipelineEntry::subcommand(
        "doctor check ir merge-truncation",
        "tungsten doctor check ir merge-truncation <dir> [--strict]",
        "Flag truncating merge memcpys feeding a phi — the 1.7.26e §6.6\n\
         phi-poisoning signature (ADR 2.7.26b)",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "doctor check ir sret-stores",
        "tungsten doctor check ir sret-stores <dir> [--strict]",
        "Lint sret returns: every ret void must store through or forward the\n\
         sret param — bare returns discard the result (ADR 3.7.26d)",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "doctor check ir wrapper-self-calls",
        "tungsten doctor check ir wrapper-self-calls <dir> [--strict]",
        "Flag depot-instance wrapper self-calls: a saturated generic self-call\n\
         re-entering its own closure wrapper @<W> instead of @<W>$direct —\n\
         per-step env alloc. The call must RETURN the closure pair { ptr, ptr }:\n\
         an arity-1 instance's saturated self-call returns its result aggregate\n\
         and is correct (ADRs 23.7.26c, 28.7.26e §2.3)",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::make_target(
        "make check-ir-audits",
        "Emit src/compiler/main.tg's IR once (~2,055 .ll) and run all six audits\n\
         over that one fresh corpus in --strict, aggregating exit codes",
    )
    .with_cost(CostTier::Compile)
    .requiring_codegen(),
    PipelineEntry::make_target(
        "make check-indirect-buffers",
        "Single-audit fast path for ABI work, sharing the same emit recipe and\n\
         corpus dir (IR_AUDIT_LL_DIR)",
    )
    .with_cost(CostTier::Compile)
    .requiring_codegen(),
    // Rendered verbatim, so the leading spaces below are the output's indentation
    // rather than source formatting — do not re-indent them.
    PipelineEntry::note(
        "
  All six dir-scanning ir audits share one contract (ADR 28.7.26e): a `summary:`
  line with a candidate/tracked reach pair, `--strict` to fail a vacuous pass
  (candidates found, nothing parsed — the audit's parser has drifted from
  emitted IR), and exit codes 0 clean / 1 findings / 2 bad input (non-directory,
  empty corpus, or vacuity under --strict). Three of the six were red the first
  time anyone ran them over the compiler's own output; every finding was false.",
    ),
];
