//! Structured musttail decision records (ADR 1.7.26b).
//!
//! Whether a self-recursive tail call actually gets `musttail` TCO is a
//! codegen-time ABI decision (`check_musttail_abi_safety`). This module is the
//! **structured integration contract** for that decision: at the point the gate
//! decides, codegen pushes a [`MusttailDecision`] into a [`MusttailReport`] sink
//! on `CodeGen`. `--trace-musttail` and the `tco-coverage` /
//! `musttail-eligibility` diagnostics are all *renderers* of these records —
//! none of them scrape formatted trace text.
//!
//! Stable `code()` strings are the machine contract asserted by `--json` and
//! snapshots; `human()` display text may improve freely without breaking tests.

/// Stable reason code for a single musttail ABI blocker.
///
/// The `code()` string is the machine-stable contract (ADR 1.7.26b §2.1) and
/// must not change without an ADR. `human()` is display-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasonCode {
    /// The lowered return type is a by-value struct.
    StructReturn,
    /// A parameter is a struct that *is* field-flattenable (decomposable —
    /// the `$direct_mt` path may still achieve musttail, see [`Decision`]).
    StructParam,
    /// A parameter is a struct that is **not** flattenable (nested struct /
    /// array fields, or too many fields) — a hard musttail blocker.
    NonFlattenableParam,
    /// Caller/callee lowered signatures differ (function-type mismatch).
    AbiSignatureMismatch,
}

impl ReasonCode {
    /// Machine-stable code string for `--json` / snapshots.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            ReasonCode::StructReturn => "STRUCT_RETURN",
            ReasonCode::StructParam => "STRUCT_PARAM",
            ReasonCode::NonFlattenableParam => "NON_FLATTENABLE_PARAM",
            ReasonCode::AbiSignatureMismatch => "ABI_SIGNATURE_MISMATCH",
        }
    }

    /// Human display text (rendering only — may change freely).
    #[must_use]
    pub fn human(self) -> &'static str {
        match self {
            ReasonCode::StructReturn => "struct return",
            ReasonCode::StructParam => "struct parameter",
            ReasonCode::NonFlattenableParam => "struct parameter (non-flattenable)",
            ReasonCode::AbiSignatureMismatch => "function-type mismatch",
        }
    }
}

/// How one source parameter is lowered into the `$direct_mt` internal entry
/// (ADR 1.7.26e). Surfaced by `info codegen indirect-abi`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamAbiKind {
    /// Passed by value unchanged — scalar, `ptr`, or recursive-ADT (`Mu`).
    ByValue,
    /// Flattenable struct decomposed into `n` scalar field args (18.5.26a).
    Decomposed(u32),
    /// Non-flattenable struct passed by a caller-owned buffer `ptr` (Class P).
    Indirect,
}

impl ParamAbiKind {
    /// Machine-stable code string for `--json`.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            ParamAbiKind::ByValue => "by-value",
            ParamAbiKind::Decomposed(_) => "decomposed",
            ParamAbiKind::Indirect => "indirect",
        }
    }

    /// Human display, e.g. `by-value`, `decomposed(3 scalars)`, `indirect (buffer ptr)`.
    #[must_use]
    pub fn human(self) -> String {
        match self {
            ParamAbiKind::ByValue => "by-value".to_string(),
            ParamAbiKind::Decomposed(n) => format!("decomposed ({n} scalars)"),
            ParamAbiKind::Indirect => "indirect (caller buffer ptr)".to_string(),
        }
    }
}

/// Position of a musttail ABI blocker within the lowered signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockerPosition {
    /// The function return type.
    Return,
    /// Parameter at the given (env-inclusive) index.
    Param(usize),
}

impl BlockerPosition {
    /// Human display, e.g. `return` or `param 1`.
    #[must_use]
    pub fn human(self) -> String {
        match self {
            BlockerPosition::Return => "return".to_string(),
            BlockerPosition::Param(i) => format!("param {i}"),
        }
    }
}

/// A single ABI blocker at a specific position in the lowered signature.
#[derive(Debug, Clone)]
pub struct Blocker {
    /// Where in the signature the blocker sits.
    pub position: BlockerPosition,
    /// Why this position blocks musttail.
    pub reason: ReasonCode,
    /// The lowered source-type display for this position (e.g. `{ i32, [56 x i8] }`).
    pub lowered_type: String,
}

/// Outcome of a musttail decision at one tail-call site.
///
/// `Decompose` is the flattenable-struct-param path (ADR 18.5.26a): the base
/// `$direct` entry cannot musttail, but a `$direct_mt` decomposed entry does.
/// For coverage purposes `Decompose` achieves constant stack, so it is treated
/// as an EMIT-class outcome by [`MusttailReport::function_outcome`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// `musttail` was emitted directly.
    Emit,
    /// The self-recursive tail call was skipped (grows stack).
    Skip,
    /// Musttail achieved via the decomposed `$direct_mt` entry.
    Decompose,
    /// A tail call to a *different* function — `musttail` was never attempted,
    /// because mutual-tail `musttail` is unimplemented (ADR 1.7.26e Non-Goal
    /// #3). Recorded, not gated (ADR 5.8.26a D4).
    ///
    /// This is deliberately NOT [`Decision::Skip`]. A `Skip` means "this
    /// self-recursive function grows its stack", which the `tco-coverage`
    /// inventory ranks and the `--gate` fails on. A non-self tail edge means
    /// neither: measured on `src/compiler/main.tg` there are **1,286 such
    /// sites across 637 distinct callees**, nearly all to functions that do not
    /// recurse at all, and folding them into `Skip` would have ranked every
    /// callee taking a `List`/`Nat`/`String` as a HIGH-risk O(N)-stack SKIP —
    /// turning the gate permanently red over a case it cannot act on.
    ///
    /// What this variant buys is that a mutual edge is *countable*. Before it,
    /// the branch was trace-only and an `f→g→f` Class-P cycle contributed zero
    /// rows, so the gate iterated a row set that could not contain it and
    /// printed `✓`. Deciding whether a given edge closes a recursion cycle
    /// needs the call graph (`doctor audit-recursion` computes one; codegen has
    /// none), and that join is the unbuilt prerequisite — see
    /// `bootstrap/src/compile/tco/gate.rs`.
    SkipNonSelf,
}

impl Decision {
    /// Machine-stable code string for `--json` / snapshots.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Decision::Emit => "EMIT",
            Decision::Skip => "SKIP",
            Decision::Decompose => "DECOMPOSE",
            Decision::SkipNonSelf => "SKIP_NON_SELF",
        }
    }

    /// Whether this outcome achieves constant stack (EMIT or DECOMPOSE).
    #[must_use]
    pub fn is_constant_stack(self) -> bool {
        matches!(self, Decision::Emit | Decision::Decompose)
    }

    /// Whether this decision concerns a **self**-recursive tail call — the
    /// population `tco-coverage` ranks and `--gate` judges. False only for
    /// [`Decision::SkipNonSelf`].
    #[must_use]
    pub fn is_self_recursive(self) -> bool {
        !matches!(self, Decision::SkipNonSelf)
    }
}

/// One structured musttail decision, emitted at the codegen gate for a single
/// tail-call site.
#[derive(Debug, Clone)]
pub struct MusttailDecision {
    /// Resolved function symbol name (e.g. `collect_type_names$direct`).
    pub function: String,
    /// The decision outcome.
    pub decision: Decision,
    /// All ABI reasons that applied (a site can hit more than one, e.g. struct
    /// param *and* return). Empty for `Emit`.
    pub reasons: Vec<ReasonCode>,
    /// Per-position blocker detail. Empty for `Emit`.
    pub blockers: Vec<Blocker>,
    /// Lowered LLVM signature display, e.g. `{ i32, [56 x i8] }(ptr, {…}, ptr)`.
    pub lowered_sig: String,
    /// Per-source-param indirect-ABI lowering (ADR 1.7.26e), in source order.
    /// Populated for `$direct_mt` (DECOMPOSE) entries; empty for plain EMIT/SKIP.
    pub param_abi: Vec<ParamAbiKind>,
    /// Whether the `$direct_mt` entry uses an sret result-out pointer.
    pub sret: bool,
    /// Per-slot `<role>: <attrs>` descriptions of the canonical lowered
    /// signature, in slot order (ADR 17.7.26e). Populated for `$direct_mt`
    /// (DECOMPOSE) entries; empty for plain EMIT/SKIP, which have no lowered
    /// slot descriptor.
    pub slot_attrs: Vec<String>,
}

impl MusttailDecision {
    /// Strip the `$direct` / `$direct_mt` suffix to recover the source-level
    /// function name used for joining against the recursion call graph.
    #[must_use]
    pub fn base_name(&self) -> &str {
        let n = &self.function;
        for suffix in ["$direct_mt", "$direct"] {
            if let Some(base) = n.strip_suffix(suffix) {
                return base;
            }
        }
        n
    }
}

impl<'ctx> super::CodeGen<'ctx> {
    /// All structured musttail decisions collected during this run (ADR 1.7.26b).
    #[must_use]
    pub fn musttail_decisions(&self) -> &[MusttailDecision] {
        self.musttail_report.decisions()
    }

    /// Drain the collected musttail decisions, leaving the report empty.
    #[must_use]
    pub fn take_musttail_decisions(&mut self) -> Vec<MusttailDecision> {
        self.musttail_report.take()
    }
}

/// A sink of musttail decisions collected during one codegen run.
///
/// Lives on `CodeGen`; decisions are pushed at the `try_emit_direct_musttail`
/// gate and its decomposed sibling. Read out after compilation via
/// `CodeGen::musttail_decisions` / `take_musttail_decisions`.
#[derive(Debug, Default)]
pub struct MusttailReport {
    decisions: Vec<MusttailDecision>,
}

impl MusttailReport {
    /// Create an empty report.
    #[must_use]
    pub fn new() -> Self {
        Self {
            decisions: Vec::new(),
        }
    }

    /// Record a decision.
    pub fn push(&mut self, decision: MusttailDecision) {
        self.decisions.push(decision);
    }

    /// All recorded decisions in emission order.
    #[must_use]
    pub fn decisions(&self) -> &[MusttailDecision] {
        &self.decisions
    }

    /// Drain the recorded decisions, leaving the report empty.
    #[must_use]
    pub fn take(&mut self) -> Vec<MusttailDecision> {
        std::mem::take(&mut self.decisions)
    }

    /// Aggregate outcome for a base function name across all its recorded sites:
    /// `Emit`/`Decompose` if *any* site achieves constant stack, else `Skip`.
    ///
    /// This is intentionally **best-decision-wins** on the constant-stack axis:
    /// a flattenable-param function whose `$direct` entry skips but whose
    /// `$direct_mt` entry decomposes is *safe*, so it must not rank as a risk
    /// (ADR 1.7.26b §2.1, reconciled with the `flat_param_emit` fixture).
    ///
    /// [`Decision::SkipNonSelf`] sites are skipped entirely (ADR 5.8.26a): this
    /// answers "does this **self-recursive** function achieve constant stack?",
    /// and a tail call *into* a function says nothing about whether that
    /// function recurses. Counting one would report `Skip` for the 637 callees
    /// that are merely tail-call targets.
    #[must_use]
    pub fn function_outcome(&self, base: &str) -> Option<Decision> {
        let mut saw_skip = false;
        for d in &self.decisions {
            if d.base_name() != base || !d.decision.is_self_recursive() {
                continue;
            }
            if d.decision.is_constant_stack() {
                return Some(d.decision); // any safe entry ⇒ constant stack
            }
            saw_skip = true;
        }
        saw_skip.then_some(Decision::Skip)
    }
}

#[cfg(test)]
mod tests;
