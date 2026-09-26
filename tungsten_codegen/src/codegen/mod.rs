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
pub(crate) use data::AllocClass;
mod debug_info;
mod definitions;
mod exec;
pub mod musttail_report;
mod naming;
mod registration;
// Concern sub-structs of `CodeGen` (ADR 4.5.26c) — grouped mutable state.
mod state;
pub(crate) use state::{
    CompilationState, DefinitionRegistry, DirectCallState, NamingState, TracingState,
};

pub use backend::CodeGenError;
use exec::polymorphism::MonomorphState;

use crate::types::TypeLowering;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{InitializationConfig, Target, TargetMachine};
use inkwell::types::BasicTypeEnum;
use inkwell::AddressSpace;

// The structural-comparator synthesis callback (ADR 29.6.26f P6′ step 2) is
// defined alongside its use in `exec::polymorphism::comparator`.
pub use exec::polymorphism::ComparatorSynth;

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
    pub(crate) direct_calls: DirectCallState<'ctx>,
    pub(crate) monomorph: MonomorphState,
    pub(crate) naming: NamingState,
    pub(crate) tracing: TracingState<'ctx>,

    /// Structured musttail decisions collected during this run (ADR 1.7.26b).
    pub(crate) musttail_report: musttail_report::MusttailReport,
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
            musttail_report: musttail_report::MusttailReport::new(),
        };

        cg.declare_runtime_functions();
        cg
    }

    /// Declare runtime functions (printf, `__tungsten_alloc`, memcpy, etc.)
    fn declare_runtime_functions(&mut self) {
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let i8_ptr = self.context.ptr_type(AddressSpace::default());

        // printf(const char*, ...) -> int
        let printf_type = i32_type.fn_type(&[i8_ptr.into()], true);
        if self.module.get_function("printf").is_none() {
            self.module.add_function("printf", printf_type, None);
        }

        // __tungsten_alloc(size: i64, class: i32) -> void* — the runtime's one
        // allocation symbol (ADR 14.9.26b), the only entry to the arena and
        // the profiler.
        let alloc_type = i8_ptr.fn_type(&[i64_type.into(), i32_type.into()], false);
        if self.module.get_function(data::ALLOC_SYMBOL).is_none() {
            self.module
                .add_function(data::ALLOC_SYMBOL, alloc_type, None);
        }

        // malloc(size: i64) -> void* and the mode flag it is gated on: mode
        // off calls malloc directly, skipping a call level (ADR 18.9.26c).
        let malloc_type = i8_ptr.fn_type(&[i64_type.into()], false);
        if self.module.get_function(data::MALLOC_SYMBOL).is_none() {
            self.module
                .add_function(data::MALLOC_SYMBOL, malloc_type, None);
        }
        // `hidden`: every program links the runtime statically, so the flag
        // is in the same image and the load needs no GOT indirection — which
        // measured as most of the branch's residual cost on the off path.
        if self.module.get_global(data::ARENA_MODE_GLOBAL).is_none() {
            let mode = self
                .module
                .add_global(i32_type, None, data::ARENA_MODE_GLOBAL);
            mode.set_visibility(inkwell::GlobalVisibility::Hidden);
        }

        // __tungsten_arena_init() -> void — the prologue reads TUNGSTEN_ARENA.
        let arena_init_type = self.context.void_type().fn_type(&[], false);
        if self.module.get_function(data::ARENA_INIT_SYMBOL).is_none() {
            self.module
                .add_function(data::ARENA_INIT_SYMBOL, arena_init_type, None);
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
