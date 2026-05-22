//! LLVM Code Generation
//!
//! Generates LLVM IR from Tungsten Core terms.
//!
//! # Module Structure
//!
//! - `error`: Error types for code generation
//! - `backend`: LLVM output functions (object file, IR dump)
//! - `data/`: Data type compilation
//!   - `primitives`: Basic types (bool, nat, unit)
//!   - `products`: Product types (pairs, tuples)
//!   - `sums`: Sum types, μ-types, fold/unfold
//!   - `adt`: Algebraic data types (flat enum representation)
//!   - `strings`: String operations
//!   - `refs`: Mutable references
//!   - `mu_types`: Recursive type helpers
//!   - `nat_ops`: Natural number arithmetic and comparisons
//!   - `bool_ops`: Boolean logic operations
//! - `exec/`: Execution-related compilation
//!   - `closures`: Lambda compilation and closure conversion
//!   - `control`: Control flow (if, natrec)
//!   - `polymorphism`: Type abstraction and monomorphization
//!   - `inference`: Type inference for code generation
//!   - `globals`: Global references and extern calls

mod abi;
mod backend;
mod compilation;
mod data;
mod debug_info;
mod definitions;
mod exec;
mod naming;
mod registration;

pub use backend::CodeGenError;

use crate::types::TypeLowering;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{InitializationConfig, Target, TargetMachine};
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, FunctionValue};
use inkwell::AddressSpace;
use std::collections::HashMap;
use std::collections::HashSet;
use tungsten_core::terms::{Term, Var};
use tungsten_core::types::Type;

// ── Concern sub-structs (ADR 4.5.26c) ──────────────────────────────

/// State for uncurried direct calling convention (ADR 2.5.26b).
pub(crate) struct DirectCallState {
    /// Known arities for top-level functions with direct entry points.
    pub(crate) arities: HashMap<String, usize>,
    /// Name of the direct entry point currently being compiled.
    pub(crate) current_entry: Option<String>,
    /// Decomposition maps for musttail-eligible functions (ADR 18.5.26a).
    /// Maps base function name → per-param decomposition info.
    /// `Some(n)` = struct flattened into n scalar fields, `None` = passthrough.
    pub(crate) decompose_maps: HashMap<String, Vec<Option<u32>>>,
}

impl DirectCallState {
    fn new() -> Self {
        Self {
            arities: HashMap::new(),
            current_entry: None,
            decompose_maps: HashMap::new(),
        }
    }
}

/// State for monomorphization of polymorphic functions.
pub(crate) struct MonomorphState {
    /// Already monomorphized function instances.
    pub(crate) instances: HashMap<(String, String), String>,
    /// Functions currently being monomorphized (cycle prevention).
    pub(crate) in_progress: HashSet<(String, String)>,
    /// When true, the single-owner mono pipeline (ADR 8.5.26g) is active.
    /// Ad-hoc per-unit monomorphization must not generate fresh instances;
    /// all mono symbols must come from the pre-seeded ownership map.
    pub(crate) mono_map_active: bool,
}

impl MonomorphState {
    fn new() -> Self {
        Self {
            instances: HashMap::new(),
            in_progress: HashSet::new(),
            mono_map_active: false,
        }
    }
}

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
}

impl NamingState {
    fn new() -> Self {
        Self {
            counter: 0,
            lambda_counter: 0,
            named_lambdas: false,
            current_binding_name: None,
            symbol_map: Vec::new(),
            module_prefix: None,
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
}

impl TracingState<'_> {
    fn new() -> Self {
        Self {
            debug_info: None,
            trace_adt_ops: None,
            alloc_profile: false,
            alloc_profile_filter: None,
            trace_musttail: false,
            trace_escape: false,
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
    fn new() -> Self {
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
    /// Variables whose sole remaining use is the current expression (last-use).
    /// Populated when entering a `Let` whose body uses the bound var exactly once.
    pub(crate) last_use_vars: HashSet<String>,
    /// Variables bound to heap-allocated string results (e.g., `StrConcat`).
    /// Only these are safe to pass to `tg_string_concat_owned` (realloc path).
    pub(crate) heap_origin_vars: HashSet<String>,
}

impl CompilationState<'_> {
    fn new() -> Self {
        Self {
            current_fn: None,
            env: HashMap::new(),
            in_tail_position: false,
            expected_lambda_ret_type: None,
            last_use_vars: HashSet::new(),
            heap_origin_vars: HashSet::new(),
        }
    }
}

// ── Main struct ─────────────────────────────────────────────────────

/// LLVM code generator for Tungsten.
///
/// # Name Resolution Paths
///
/// The codegen layer resolves function names through 4 distinct paths,
/// consulted in different contexts:
///
/// 1. **`extern_name_map`** — Maps original definition names to their LLVM symbol
///    names (e.g., `tg_argc` → `__wrap_tg_argc`, or colliding names like
///    `helper` → `alpha__helper`). Checked first during global references and
///    direct calls. Populated during declaration phase.
///
/// 2. **`def_types`** — Maps LLVM name → Core type. Used by monomorphization to
///    discover which functions are polymorphic (`Forall`) and need specialization.
///    Both original and scoped names may be registered for colliding definitions.
///
/// 3. **`term_defs`** — Maps LLVM name → Core term body. Used by monomorphization
///    to compile specialized instances on-demand. Same dual-registration as
///    `def_types` for colliding names.
///
/// 4. **`module.get_function(name)`** — LLVM module-level lookup for functions
///    already declared or defined. Used as a fallback when resolving cross-module
///    references and checking if a function prototype already exists.
pub struct CodeGen<'ctx> {
    // LLVM infrastructure (unchanged — too fundamental to wrap)
    pub(crate) context: &'ctx Context,
    pub(crate) module: Module<'ctx>,
    pub(crate) builder: Builder<'ctx>,
    pub(crate) types: TypeLowering<'ctx>,

    // Per-function state (changes during compilation)
    pub(crate) compilation: CompilationState<'ctx>,

    // Definition registry (populated during declaration, read during compilation)
    pub(crate) defs: DefinitionRegistry,

    // Grouped concerns
    pub(crate) direct_calls: DirectCallState,
    pub(crate) monomorph: MonomorphState,
    pub(crate) naming: NamingState,
    pub(crate) tracing: TracingState<'ctx>,
}

/// An entry mapping an IR function name to its source-level name and location.
#[derive(Debug, Clone)]
pub struct SymbolEntry {
    /// The name used in LLVM IR (e.g., `__lambda_42` or `filter_trivia_acc`)
    pub ir_name: String,
    /// The source-level binding name, if known (e.g., `filter_trivia_acc`)
    pub source_name: Option<String>,
    /// Source file path, if known
    pub file: Option<String>,
    /// Source line number, if known
    pub line: Option<u32>,
}

impl<'ctx> CodeGen<'ctx> {
    /// Create a new code generator.
    #[must_use]
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        let mut types = TypeLowering::new(context);

        // Initialize native target for correct ABI handling
        Target::initialize_native(&InitializationConfig::default())
            .expect("Failed to initialize native target");

        // Set target triple and data layout on module for proper ARM64 ABI
        // Also extract TargetData for accurate type size calculation with alignment
        let target_triple = TargetMachine::get_default_triple();
        module.set_triple(&target_triple);
        if let Ok(target) = Target::from_triple(&target_triple) {
            if let Some(target_machine) = target.create_target_machine(
                &target_triple,
                "generic",
                "",
                inkwell::OptimizationLevel::Default,
                inkwell::targets::RelocMode::PIC,
                inkwell::targets::CodeModel::Default,
            ) {
                let td = target_machine.get_target_data();
                module.set_data_layout(&td.get_data_layout());
                // Pass TargetData to TypeLowering for accurate size calculation
                types.set_target_data(td);
            }
        }

        let mut cg = Self {
            context,
            module,
            builder,
            types,
            compilation: CompilationState::new(),
            defs: DefinitionRegistry::new(),
            direct_calls: DirectCallState::new(),
            monomorph: MonomorphState::new(),
            naming: NamingState::new(),
            tracing: TracingState::new(),
        };

        cg.declare_runtime_functions();
        cg
    }

    /// Declare runtime functions (printf, malloc, etc.)
    fn declare_runtime_functions(&mut self) {
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let i8_ptr = self.context.ptr_type(AddressSpace::default());

        // printf(const char*, ...) -> int
        let printf_type = i32_type.fn_type(&[i8_ptr.into()], true);
        if self.module.get_function("printf").is_none() {
            self.module.add_function("printf", printf_type, None);
        }

        // malloc(size_t) -> void*
        let malloc_type = i8_ptr.fn_type(&[i64_type.into()], false);
        if self.module.get_function("malloc").is_none() {
            self.module.add_function("malloc", malloc_type, None);
        }

        // memcpy(dest, src, n) -> dest
        let memcpy_type = i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into(), i64_type.into()], false);
        if self.module.get_function("memcpy").is_none() {
            self.module.add_function("memcpy", memcpy_type, None);
        }

        // memcmp(s1, s2, n) -> int
        let memcmp_type = i32_type.fn_type(&[i8_ptr.into(), i8_ptr.into(), i64_type.into()], false);
        if self.module.get_function("memcmp").is_none() {
            self.module.add_function("memcmp", memcmp_type, None);
        }

        // tg_string_concat({ptr, i64}, {ptr, i64}) -> {ptr, i64}
        let string_type = self
            .context
            .struct_type(&[i8_ptr.into(), i64_type.into()], false);
        let concat_type = string_type.fn_type(&[string_type.into(), string_type.into()], false);
        if self.module.get_function("tg_string_concat").is_none() {
            self.module
                .add_function("tg_string_concat", concat_type, None);
        }

        // tg_string_concat_owned({ptr, i64}, {ptr, i64}) -> {ptr, i64}
        // Same signature — left is consumed (caller guarantees it's dead)
        if self.module.get_function("tg_string_concat_owned").is_none() {
            self.module
                .add_function("tg_string_concat_owned", concat_type, None);
        }
    }

    /// Get the malloc function to use for allocations.
    ///
    /// When allocation profiling is enabled, returns `__tungsten_alloc_profile_malloc`
    /// (which wraps malloc and records per-function attribution).
    /// Otherwise returns the standard `malloc`.
    pub(crate) fn get_malloc(&self) -> inkwell::values::FunctionValue<'ctx> {
        if self.tracing.alloc_profile {
            self.module
                .get_function("__tungsten_alloc_profile_malloc")
                .expect("alloc profiler malloc not declared (call set_alloc_profile first)")
        } else {
            self.module
                .get_function("malloc")
                .expect("malloc not declared")
        }
    }

    /// Get the LLVM module.
    pub fn module(&self) -> &Module<'ctx> {
        &self.module
    }

    /// Get size of type in bytes using LLVM `TargetData` for accurate alignment.
    /// Delegates to `TypeLowering::type_size()` which holds the `TargetData`.
    pub(crate) fn type_size_bytes(&self, ty: BasicTypeEnum<'ctx>) -> u64 {
        self.types.type_size(ty)
    }
}

#[cfg(test)]
mod tests;
