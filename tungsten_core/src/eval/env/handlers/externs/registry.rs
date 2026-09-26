//! The externs the evaluator can actually execute — one place, so tooling and
//! dispatch cannot disagree.
//!
//! ## Why this exists
//!
//! The evaluator executes only the externs it has a match arm for; every other
//! `ExternCall` goes **silently `Stuck`** — no error, no warning, no output.
//! That silence is the trap: ADR 28.7.26a found `println` had been unreachable
//! on the evaluator the whole time, because it is not one extern but a chain of
//! three, each individually plausible. A `.tg` program using it simply printed
//! nothing.
//!
//! The dispatch arms live in [`super::call`] and
//! [`super::console`] and cannot move here — they carry real
//! marshalling logic. What *can* live here is the answer to "which names does
//! the evaluator know?", which is what `tungsten info eval externs` and
//! `tungsten doctor check extern-coverage` report.
//!
//! ## Keeping it honest
//!
//! `tests::every_registry_entry_is_claimed_by_dispatch` drives every entry
//! below through the real dispatchers, so an entry describing an extern that
//! does not exist fails the build. The reverse direction — a new match arm with
//! no entry here — makes the diagnostics *under*-report, which is the safe way
//! round but still wrong: **add the entry in the same change as the arm.**

/// What kind of work an executable extern does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternKind {
    /// A `tungsten test` assertion or harness hook.
    TestAssertion,
    /// Console output: writes to the process streams, or to an installed
    /// capture sink (ADR 28.7.26a §2.2).
    Console,
    /// A pure computation, safe to run during evaluation.
    Pure,
    /// Reads or writes the thread-local type arena (ADR 7.8.26c §2.4).
    ///
    /// Distinct from [`Self::Pure`] in both directions: a constructor
    /// *allocates*, and even a read depends on arena state, so neither is the
    /// self-contained computation `tg_string_compare` is. Filing them under
    /// `Pure` would make `doctor check extern-coverage` — which reports by
    /// kind — say the evaluator executes six more pure functions than it does.
    Arena,
    /// Mutates a `StringBuilder` behind an opaque handle (ADR 14.9.26a).
    ///
    /// Its own kind for the same reason as [`Self::Arena`]: every arm
    /// allocates or mutates heap state the handle names, so none is the
    /// self-contained computation `Pure` promises — and, unlike the arena, a
    /// consumed handle *aborts*, which a report reader deserves to see
    /// labelled.
    Builder,
}

impl ExternKind {
    /// A short label for report output.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::TestAssertion => "test-assertion",
            Self::Console => "console",
            Self::Pure => "pure",
            Self::Arena => "arena",
            Self::Builder => "builder",
        }
    }
}

/// One extern the evaluator executes.
#[derive(Debug, Clone, Copy)]
pub struct ExecutableExtern {
    /// The C symbol name, as written in a `.tg` `extern "C" fn` declaration.
    pub name: &'static str,
    /// What kind of work it does.
    pub kind: ExternKind,
    /// One line on what evaluating it does.
    pub summary: &'static str,
}

/// Every extern the evaluator can execute.
///
/// Anything absent from this list goes silently `Stuck` when evaluated —
/// `tungsten run`/`test` will produce no error, just no effect.
pub const EXECUTABLE_EXTERNS: &[ExecutableExtern] = &[
    ExecutableExtern {
        name: "tg_assert_eq_nat",
        kind: ExternKind::TestAssertion,
        summary: "Assert two Nats are equal; records a failure on mismatch",
    },
    ExecutableExtern {
        name: "tg_assert_eq_bool",
        kind: ExternKind::TestAssertion,
        summary: "Assert two Bools are equal; records a failure on mismatch",
    },
    ExecutableExtern {
        name: "tg_assert_eq_int",
        kind: ExternKind::TestAssertion,
        summary: "Assert two Ints are equal; records a failure on mismatch",
    },
    ExecutableExtern {
        name: "tg_assert_eq_string",
        kind: ExternKind::TestAssertion,
        summary:
            "Assert two Strings are equal, as (address, len) pairs; records a failure on mismatch",
    },
    ExecutableExtern {
        name: "tg_test_check_failure",
        kind: ExternKind::TestAssertion,
        summary: "Read and clear the per-test failure flag",
    },
    ExecutableExtern {
        name: "tg_string_compare",
        kind: ExternKind::Pure,
        summary: "Three-way byte-wise string comparison (0/1/2)",
    },
    ExecutableExtern {
        name: "tg_string_char_at_internal",
        kind: ExternKind::Pure,
        summary: "Byte at an index (0 out of range) — the base case `string_eq` bottoms out in",
    },
    ExecutableExtern {
        name: "tg_string_to_cstring",
        kind: ExternKind::Console,
        summary: "Allocate a null-terminated C copy of a String; returns its address",
    },
    ExecutableExtern {
        name: "tg_string_len_internal",
        kind: ExternKind::Console,
        summary: "Byte length of a String",
    },
    ExecutableExtern {
        name: "tg_free_string",
        kind: ExternKind::Console,
        summary: "Release a C string from tg_string_to_cstring",
    },
    ExecutableExtern {
        name: "tg_print",
        kind: ExternKind::Console,
        summary: "Write bytes to stdout (no newline), or to an installed capture sink",
    },
    ExecutableExtern {
        name: "tg_println",
        kind: ExternKind::Console,
        summary: "Write bytes plus a newline to stdout, or to an installed capture sink",
    },
    ExecutableExtern {
        name: "tg_eprintln",
        kind: ExternKind::Console,
        summary: "Write bytes plus a newline to stderr, or to an installed capture sink",
    },
    // ── The type arena (ADR 7.8.26c) ───────────────────────────────────────
    //
    // These twenty are what `tungsten info eval reachable-externs` names on
    // the six `test_type_handle_to_codegen_type_*` paths. The list is the
    // TOOL's answer, not a hand count: it walks the static call graph, so the
    // five accessors no test actually triggers (product ×2, forall ×2, tyvar)
    // are on the path too and had to be claimed for `blocking` to reach 0.
    //
    // Their *constructors* are deliberately absent — nothing reaches
    // `tg_type_product`/`_forall`/`_var` — as is `tg_init`, which resets the
    // arena and would let one test body invalidate another's handles.
    ExecutableExtern {
        name: "tg_type_nat",
        kind: ExternKind::Arena,
        summary: "Allocate the Nat type node; returns its handle",
    },
    ExecutableExtern {
        name: "tg_type_bool",
        kind: ExternKind::Arena,
        summary: "Allocate the Bool type node; returns its handle",
    },
    ExecutableExtern {
        name: "tg_type_unit",
        kind: ExternKind::Arena,
        summary: "Allocate the Unit type node; returns its handle",
    },
    ExecutableExtern {
        name: "tg_type_arrow",
        kind: ExternKind::Arena,
        summary: "Allocate an arrow type from a domain and codomain handle",
    },
    ExecutableExtern {
        name: "tg_type_sum",
        kind: ExternKind::Arena,
        summary: "Allocate a sum type from a left and right handle",
    },
    ExecutableExtern {
        name: "tg_type_mu",
        kind: ExternKind::Arena,
        summary: "Allocate a μ type from a C-string binder name and a body handle",
    },
    ExecutableExtern {
        name: "tg_type_tag",
        kind: ExternKind::Arena,
        summary: "The variant tag of a type handle — the table `tg_type_tag` itself defines",
    },
    ExecutableExtern {
        name: "tg_type_get_arrow_domain",
        kind: ExternKind::Arena,
        summary: "Domain of an arrow type, or INVALID_HANDLE if it is not one",
    },
    ExecutableExtern {
        name: "tg_type_get_arrow_codomain",
        kind: ExternKind::Arena,
        summary: "Codomain of an arrow type, or INVALID_HANDLE if it is not one",
    },
    ExecutableExtern {
        name: "tg_type_get_sum_left",
        kind: ExternKind::Arena,
        summary: "Left component of a sum type, or INVALID_HANDLE if it is not one",
    },
    ExecutableExtern {
        name: "tg_type_get_sum_right",
        kind: ExternKind::Arena,
        summary: "Right component of a sum type, or INVALID_HANDLE if it is not one",
    },
    ExecutableExtern {
        name: "tg_type_get_product_left",
        kind: ExternKind::Arena,
        summary: "Left component of a product type, or INVALID_HANDLE if it is not one",
    },
    ExecutableExtern {
        name: "tg_type_get_product_right",
        kind: ExternKind::Arena,
        summary: "Right component of a product type, or INVALID_HANDLE if it is not one",
    },
    ExecutableExtern {
        name: "tg_type_get_mu_body",
        kind: ExternKind::Arena,
        summary: "Body of a μ type, or INVALID_HANDLE if it is not one",
    },
    ExecutableExtern {
        name: "tg_type_get_forall_body",
        kind: ExternKind::Arena,
        summary: "Body of a forall type, or INVALID_HANDLE if it is not one",
    },
    ExecutableExtern {
        name: "tg_type_get_mu_var",
        kind: ExternKind::Arena,
        summary: "Binder name of a μ type, as the address of a leaked C string",
    },
    ExecutableExtern {
        name: "tg_type_get_forall_var",
        kind: ExternKind::Arena,
        summary: "Binder name of a forall type, as the address of a leaked C string",
    },
    ExecutableExtern {
        name: "tg_type_get_tyvar_name",
        kind: ExternKind::Arena,
        summary: "Name of a type variable, as the address of a leaked C string",
    },
    ExecutableExtern {
        name: "tg_string_to_cstr",
        kind: ExternKind::Arena,
        summary: "Leak a null-terminated C copy of a String; returns its address \
                  (NOT tg_string_to_cstring, a different symbol)",
    },
    ExecutableExtern {
        name: "tg_cstring_to_string",
        kind: ExternKind::Arena,
        summary: "Read the C string at an address back into a String",
    },
    // ── StringBuilder (ADR 14.9.26a) ──────────────────────────────────────
    //
    // The six externs `src/compiler/driver/ffi/builder/mod.tg` declares. A
    // handle is the address of a runtime-owned header, marshalled as Nat; the
    // evaluated and compiled paths share the one heap.
    ExecutableExtern {
        name: "tg_string_builder_new",
        kind: ExternKind::Builder,
        summary: "Allocate an empty StringBuilder; returns its handle",
    },
    ExecutableExtern {
        name: "tg_string_builder_with_capacity",
        kind: ExternKind::Builder,
        summary: "Allocate a StringBuilder with a pre-sized buffer; returns its handle",
    },
    ExecutableExtern {
        name: "tg_string_builder_push_str",
        kind: ExternKind::Builder,
        summary: "Append a String to a StringBuilder; returns the same handle",
    },
    ExecutableExtern {
        name: "tg_string_builder_push_char",
        kind: ExternKind::Builder,
        summary: "Append one Unicode scalar as UTF-8; returns the same handle \
                  (a non-scalar aborts)",
    },
    ExecutableExtern {
        name: "tg_string_builder_len",
        kind: ExternKind::Builder,
        summary: "Bytes used by a StringBuilder",
    },
    ExecutableExtern {
        name: "tg_string_builder_to_string",
        kind: ExternKind::Builder,
        summary: "Consume a StringBuilder into its String; the handle is dead afterwards",
    },
];

/// Whether the evaluator can execute `name`.
///
/// Strips the `__c_` C-ABI prefix the elaborator prepends, so a caller may pass
/// either the source spelling or the elaborated one.
#[must_use]
pub fn is_executable(name: &str) -> bool {
    entry_for(name).is_some()
}

/// The registry entry for `name`, or `None` if the evaluator would leave a call
/// to it stuck.
///
/// Named for what it returns rather than what it does: `lookup` would say only
/// that a search happens, leaving the caller to guess whether the answer is a
/// bool, an index, or the entry itself.
#[must_use]
pub fn entry_for(name: &str) -> Option<&'static ExecutableExtern> {
    let name = name.strip_prefix("__c_").unwrap_or(name);
    EXECUTABLE_EXTERNS.iter().find(|e| e.name == name)
}

// Tests: registry_tests.rs — they drive the real dispatchers, so they
// are the coupling that keeps this list from drifting.
#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
