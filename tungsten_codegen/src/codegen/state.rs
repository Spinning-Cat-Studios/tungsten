//! Concern sub-structs of [`CodeGen`](super::CodeGen) (ADR 4.5.26c): the
//! grouped mutable state the generator threads through compilation —
//! direct-call entries, naming counters, tracing switches, the definition
//! registry, and per-function compilation state. Split from `codegen/mod.rs`
//! so the parent holds only the `CodeGen` struct + construction.

use std::collections::HashMap;
use std::collections::HashSet;

use inkwell::values::{BasicValueEnum, FunctionValue};
use tungsten_core::terms::{Term, Var};
use tungsten_core::types::Type;

use super::debug_info;
use super::SymbolEntry;

/// Per-function direct-entry lowering info (ADR 2.7.26b T7). One entry per
/// base function name — formerly three parallel maps (`arities`,
/// `decompose_maps`, `lowered_sigs`) kept in sync only by convention; the
/// coupling is now structural. Fields are optional because they are populated
/// at different pipeline stages: arity at declaration, the decompose map when
/// a musttail-eligible lowering plan is computed, the lowered signature only
/// for Class-P/decomposed entries.
#[derive(Default)]
pub(crate) struct DirectEntryInfo<'ctx> {
    /// Known arity of the function's direct entry point.
    pub(crate) arity: Option<usize>,
    /// Per-param `$direct_mt` lowering plan (ADRs 18.5.26a, 1.7.26e):
    /// `Decompose(n)` = flattenable struct → n scalars, `Indirect` =
    /// non-flattenable struct → caller-owned buffer ptr, `Passthrough` =
    /// unchanged.
    pub(crate) decompose_map:
        Option<Vec<crate::codegen::exec::direct_calls::decompose::ParamLowering>>,
    /// Canonical lowered `$direct_mt` signature (ADR 1.7.26e R6). Single
    /// source of truth: the entry's LLVM function type is derived from this,
    /// its attributes are attached from it at declaration and at every call
    /// site, and the self-tail edge gates on
    /// [`LoweredSignature::musttail_compatible`](crate::codegen::abi::LoweredSignature::musttail_compatible).
    pub(crate) lowered_sig: Option<crate::codegen::abi::LoweredSignature<'ctx>>,
}

/// State for uncurried direct calling convention (ADR 2.5.26b).
pub(crate) struct DirectCallState<'ctx> {
    /// Per-function direct-entry info, keyed by base function name.
    pub(crate) entries: HashMap<String, DirectEntryInfo<'ctx>>,
    /// Name of the direct entry point currently being compiled.
    pub(crate) current_entry: Option<String>,
}

// Accessors (`set_arity`/`arity`/`set_decompose_map`/`decompose_map`/
// `set_lowered_sig`/`lowered_sig` + `new`) live with their consumers in
// `exec/direct_calls/helpers.rs`.

/// State for naming, counters, and symbol tracking.
pub(crate) struct NamingState {
    pub(crate) counter: u64,
    pub(crate) lambda_counter: u64,
    pub(crate) named_lambdas: bool,
    /// Current let-binding name, set by `compile_let` and `compile_def_with_span`.
    /// Also read by escape analysis in `compile_fold` to determine whether
    /// the fold's result is non-escaping and can use stack allocation.
    pub(crate) current_binding_name: Option<String>,
    pub(crate) symbol_map: Vec<SymbolEntry>,
    /// Per-module prefix for generated symbol names (lambdas, mono instances, fix).
    /// When set, prevents name collisions across codegen units.
    pub(crate) module_prefix: Option<String>,
    /// The symbol of the synthesized def currently being compiled, or `None`
    /// for ordinary user code (ADR 12.7.26c P6/D7). When `Some(sym)`, lambdas
    /// inside it name deterministically as `sym$l<index>` — regardless of the
    /// `--named-lambdas` flag — so a codegen error inside a synthesized body
    /// names the comparator it belongs to instead of `__<unit>_lambda_N`.
    pub(crate) synthesized_def_binding: Option<String>,
    /// Per-synthesized-def lambda index, reset at the start of each synthesized
    /// def so names are deterministic (`$l0`, `$l1`, …).
    pub(crate) synthesized_lambda_index: usize,
}

impl NamingState {
    pub(super) fn new() -> Self {
        Self {
            counter: 0,
            lambda_counter: 0,
            named_lambdas: false,
            current_binding_name: None,
            symbol_map: Vec::new(),
            module_prefix: None,
            synthesized_def_binding: None,
            synthesized_lambda_index: 0,
        }
    }
}

/// State for debug info and runtime tracing.
pub(crate) struct TracingState<'ctx> {
    pub(crate) debug_info: Option<debug_info::DebugInfoState<'ctx>>,
    pub(crate) trace_adt_ops: Option<String>,
    /// When set, emit per-function allocation profiling hooks (ADR 7.5.26b).
    pub(crate) alloc_profile: bool,
    /// Optional function name filter for the allocation profile report.
    pub(crate) alloc_profile_filter: Option<String>,
    /// When set, trace musttail decisions to stderr (ADR 8.5.26c).
    pub(crate) trace_musttail: bool,
    /// When set, trace escape analysis decisions to stderr (ADR 8.5.26d).
    pub(crate) trace_escape: bool,
    /// When `Some`, print each synthesized comparator's Core term as it is
    /// emitted (ADR 12.7.26c P6). `Some(None)` dumps every synthesized def;
    /// `Some(Some(sym))` dumps only the def whose symbol contains `sym`. These
    /// terms are invisible to `--dump-ir` (which reads elaborated defs — the
    /// P6′ intercept builds them after that point).
    pub(crate) dump_synthesized: Option<Option<String>>,
}

impl TracingState<'_> {
    pub(super) fn new() -> Self {
        Self {
            debug_info: None,
            trace_adt_ops: None,
            alloc_profile: false,
            alloc_profile_filter: None,
            trace_musttail: false,
            trace_escape: false,
            dump_synthesized: None,
        }
    }
}

/// Registry of top-level definitions available during codegen.
///
/// Groups definition types, term bodies, extern name mappings, and
/// escape analysis results — all populated before compilation and
/// read during code generation.
pub(crate) struct DefinitionRegistry {
    /// Top-level definition types: name -> type.
    pub(crate) def_types: HashMap<String, Type>,
    /// Original term definitions for monomorphization.
    pub(crate) term_defs: HashMap<String, Term>,
    /// Extern name mappings: `original_name` -> `llvm_name`.
    pub(crate) extern_name_map: HashMap<String, String>,
    /// Variables bound to non-escaping Fold results (can use alloca instead of malloc).
    pub(crate) non_escaping_folds: HashSet<String>,
}

impl DefinitionRegistry {
    pub(super) fn new() -> Self {
        Self {
            def_types: HashMap::new(),
            term_defs: HashMap::new(),
            extern_name_map: HashMap::new(),
            non_escaping_folds: HashSet::new(),
        }
    }
}

/// Per-function compilation state that changes as each function is compiled.
pub(crate) struct CompilationState<'ctx> {
    /// Current function being compiled.
    pub(crate) current_fn: Option<FunctionValue<'ctx>>,
    /// Variable bindings: name -> (value, type).
    pub(crate) env: HashMap<Var, (BasicValueEnum<'ctx>, Type)>,
    /// Whether the current expression is in tail position of its enclosing function.
    pub(crate) in_tail_position: bool,
    /// Expected return type for the innermost lambda being compiled.
    pub(crate) expected_lambda_ret_type: Option<Type>,
    /// Sret out-pointer pointee type when compiling a void-returning
    /// `$direct_mt` body (ADR 1.7.26a): an early `return` must store through
    /// param 0 rather than emit a bare `ret void` (ADR 3.7.26a).
    pub(crate) current_sret_type: Option<inkwell::types::BasicTypeEnum<'ctx>>,
    /// Variables whose sole remaining use is the current expression (last-use).
    /// Populated when entering a `Let` whose body uses the bound var exactly once.
    pub(crate) last_use_vars: HashSet<String>,
    /// Variables bound to heap-allocated string results (e.g., `StrConcat`).
    /// Only these are safe to pass to `tg_string_concat_owned` (realloc path).
    pub(crate) heap_origin_vars: HashSet<String>,
}

impl CompilationState<'_> {
    pub(super) fn new() -> Self {
        Self {
            current_fn: None,
            env: HashMap::new(),
            in_tail_position: false,
            expected_lambda_ret_type: None,
            current_sret_type: None,
            last_use_vars: HashSet::new(),
            heap_origin_vars: HashSet::new(),
        }
    }
}
