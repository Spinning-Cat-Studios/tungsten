//! Data type compilation
//!
//! Compilation of Tungsten data types to LLVM IR:
//! - `primitives`: Basic types (bool, nat, unit)
//! - `products`: Product types (pairs, tuples)
//! - `sums`: Sum types and case expressions
//! - `adt`: Algebraic data types (flat enum representation)
//! - `mu_types`: Recursive type helpers (fold/unfold)
//! - `ops`: Data type operations (bool, nat, string, ref)
//!
//! Also hosts the shared heap-allocation helper (`build_malloc_call`) and
//! the `AllocClass` tags it feeds to the runtime's one allocation symbol,
//! `__tungsten_alloc(size, class)` (ADR 14.9.26b), where the class reaches
//! the allocation profiler (ADR 2.7.26a) when one is active.

pub(crate) mod adt;
pub(crate) mod mu_types;
pub(crate) mod ops;
pub(crate) mod primitives;
pub(crate) mod products;
pub(crate) mod sums;

use super::{CodeGen, CodeGenError};

/// Allocation class tags every generated allocation carries (ADR 2.7.26a).
///
/// Discriminants MUST match the `CLASS_*` constants in
/// `tungsten_runtime/src/alloc_profile/mod.rs` — they cross the FFI
/// boundary as the second argument of `__tungsten_alloc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AllocClass {
    /// Recursive-ADT node (`fold_to_heap` / `mu_alloc`).
    Mu = 0,
    /// Closure environment (`env_alloc`).
    Env = 1,
    /// Mutable reference cell (`ref.new`).
    Ref = 2,
    /// String buffer (`malloc_bytes` in string ops).
    Str = 3,
}

/// The runtime's allocation symbol: `ptr __tungsten_alloc(i64 size, i32 class)`
/// (ADR 14.9.26b). Codegen never chooses the arena mode — the prologue's
/// [`ARENA_INIT_SYMBOL`] reads `TUNGSTEN_ARENA` at start-up.
pub(crate) const ALLOC_SYMBOL: &str = "__tungsten_alloc";

/// The prologue call that reads the mode: `void __tungsten_arena_init()`.
pub(crate) const ARENA_INIT_SYMBOL: &str = "__tungsten_arena_init";

/// The runtime-exported mode flag generated code branches on (ADR 18.9.26c):
/// `external hidden global i32`, `0` = off, written once by the prologue.
pub(crate) const ARENA_MODE_GLOBAL: &str = "__tungsten_arena_mode";

/// The platform allocator the off arm calls directly (ADR 18.9.26c).
pub(crate) const MALLOC_SYMBOL: &str = "malloc";

/// Block-name stem of the off arm. `check-arena-mode` accepts a `@malloc`
/// call only inside a block whose label starts with it (LLVM appends a
/// counter to repeated names), so the two must change together.
pub(crate) const MALLOC_BLOCK: &str = "alloc.malloc";

/// Relative weight of the off arm in the mode branch's `branch_weights`
/// (against `1` for the arena arm): `off` is the default mode.
const OFF_ARM_WEIGHT: u64 = 2000;

fn llvm_err(builder_error: impl ToString) -> CodeGenError {
    CodeGenError::LlvmError(builder_error.to_string())
}

fn into_pointer(
    call: inkwell::values::CallSiteValue<'_>,
) -> Result<inkwell::values::PointerValue<'_>, CodeGenError> {
    call.try_as_basic_value()
        .left()
        .ok_or_else(|| CodeGenError::LlvmError("malloc returned void".to_string()))
        .map(inkwell::values::BasicValueEnum::into_pointer_value)
}

impl<'ctx> CodeGen<'ctx> {
    /// Build a heap allocation, tagged with its allocation class.
    ///
    /// Non-profiled (ADR 18.9.26c): branch on `@__tungsten_arena_mode` —
    /// mode off calls `@malloc` directly, anything else calls
    /// `@__tungsten_alloc(size, class)` — so the default path pays one
    /// predicted branch instead of a call level. Leaves the builder at the end
    /// of the `alloc.join` block, which every caller continues in.
    ///
    /// Profiled (`--alloc-profile`): always calls the symbol, because the
    /// profiler records inside it and a direct `@malloc` would bypass it.
    pub(crate) fn build_malloc_call(
        &self,
        size: inkwell::values::IntValue<'ctx>,
        class: AllocClass,
        name: &str,
    ) -> Result<inkwell::values::PointerValue<'ctx>, CodeGenError> {
        if self.tracing.alloc_profile {
            return self.build_symbol_alloc(size, class, name);
        }
        self.build_mode_branched_alloc(size, class, name)
    }

    /// `call ptr @__tungsten_alloc(i64 size, i32 class)` at the builder.
    fn build_symbol_alloc(
        &self,
        size: inkwell::values::IntValue<'ctx>,
        class: AllocClass,
        name: &str,
    ) -> Result<inkwell::values::PointerValue<'ctx>, CodeGenError> {
        let alloc_fn = self
            .module
            .get_function(ALLOC_SYMBOL)
            .expect("__tungsten_alloc not declared (declare_runtime_functions)");
        let class_val = self.context.i32_type().const_int(class as u64, false);
        let call = self
            .builder
            .build_call(alloc_fn, &[size.into(), class_val.into()], name)
            .map_err(llvm_err)?;
        into_pointer(call)
    }

    /// The §2.2 shape: load the mode, branch, call either allocator, `phi`.
    fn build_mode_branched_alloc(
        &self,
        size: inkwell::values::IntValue<'ctx>,
        class: AllocClass,
        name: &str,
    ) -> Result<inkwell::values::PointerValue<'ctx>, CodeGenError> {
        let function = self
            .builder
            .get_insert_block()
            .and_then(inkwell::basic_block::BasicBlock::get_parent)
            .ok_or_else(|| CodeGenError::LlvmError("allocation outside a function".into()))?;
        let mode_global = self
            .module
            .get_global(ARENA_MODE_GLOBAL)
            .expect("__tungsten_arena_mode not declared (declare_runtime_functions)");
        let malloc_fn = self
            .module
            .get_function(MALLOC_SYMBOL)
            .expect("malloc not declared (declare_runtime_functions)");
        let i32_type = self.context.i32_type();

        let mode = self
            .builder
            .build_load(i32_type, mode_global.as_pointer_value(), "arena.mode")
            .map_err(llvm_err)?
            .into_int_value();
        let is_off = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::EQ,
                mode,
                i32_type.const_zero(),
                "arena.off",
            )
            .map_err(llvm_err)?;
        let malloc_block = self.context.append_basic_block(function, MALLOC_BLOCK);
        let arena_block = self.context.append_basic_block(function, "alloc.arena");
        let join_block = self.context.append_basic_block(function, "alloc.join");
        let branch = self
            .builder
            .build_conditional_branch(is_off, malloc_block, arena_block)
            .map_err(llvm_err)?;
        self.mark_off_arm_likely(branch)?;

        self.builder.position_at_end(malloc_block);
        let call = self
            .builder
            .build_call(malloc_fn, &[size.into()], &format!("{name}.malloc"))
            .map_err(llvm_err)?;
        let from_malloc = into_pointer(call)?;
        self.builder
            .build_unconditional_branch(join_block)
            .map_err(llvm_err)?;

        self.builder.position_at_end(arena_block);
        let from_arena = self.build_symbol_alloc(size, class, &format!("{name}.arena"))?;
        self.builder
            .build_unconditional_branch(join_block)
            .map_err(llvm_err)?;

        self.builder.position_at_end(join_block);
        let phi = self
            .builder
            .build_phi(
                self.context.ptr_type(inkwell::AddressSpace::default()),
                name,
            )
            .map_err(llvm_err)?;
        phi.add_incoming(&[(&from_malloc, malloc_block), (&from_arena, arena_block)]);
        Ok(phi.as_basic_value().into_pointer_value())
    }

    /// `!prof !{!"branch_weights", i32 OFF_ARM_WEIGHT, i32 1}` on the mode
    /// branch, so the default mode is the fall-through (ADR 18.9.26c §2.2,
    /// the first escalation AC 1 names).
    fn mark_off_arm_likely(
        &self,
        branch: inkwell::values::InstructionValue<'ctx>,
    ) -> Result<(), CodeGenError> {
        let i32_type = self.context.i32_type();
        let weights = self.context.metadata_node(&[
            self.context.metadata_string("branch_weights").into(),
            i32_type.const_int(OFF_ARM_WEIGHT, false).into(),
            i32_type.const_int(1, false).into(),
        ]);
        branch
            .set_metadata(weights, self.context.get_kind_id("prof"))
            .map_err(llvm_err)
    }
}

#[cfg(test)]
mod tests {
    use super::{AllocClass, ALLOC_SYMBOL, ARENA_INIT_SYMBOL};
    use tungsten_runtime::{CLASS_ENV, CLASS_MU, CLASS_OTHER, CLASS_REF, CLASS_STRING};

    /// The discriminants cross the FFI boundary as a raw `u32`; a mismatch
    /// silently misattributes allocation classes in the profiler.
    #[test]
    fn alloc_class_discriminants_match_runtime_constants() {
        assert_eq!(AllocClass::Mu as u32, CLASS_MU);
        assert_eq!(AllocClass::Env as u32, CLASS_ENV);
        assert_eq!(AllocClass::Ref as u32, CLASS_REF);
        assert_eq!(AllocClass::Str as u32, CLASS_STRING);
        // The runtime clamps out-of-range classes to CLASS_OTHER at record
        // time, so every codegen-emitted class must sit below the sentinel.
        assert!((AllocClass::Str as u32) < CLASS_OTHER);
    }

    /// ADR 14.9.26b AC 3: the mode enum's twin. Codegen holds no mode of its
    /// own — the prologue reads it — so the contract it pins is the runtime's:
    /// the published `MODE_*` constants are the `ArenaMode` discriminants that
    /// `__tungsten_arena_stats` writes into its `mode` field, and a process
    /// whose prologue never ran reads as `Off`.
    #[test]
    fn arena_mode_discriminants_match_runtime_constants() {
        use tungsten_runtime::{ArenaMode, MODE_BUMP, MODE_OFF};
        assert_eq!(ArenaMode::Off as u32, MODE_OFF);
        assert_eq!(ArenaMode::Bump as u32, MODE_BUMP);
        assert_ne!(MODE_OFF, MODE_BUMP);
        assert_eq!(tungsten_runtime::arena_mode(), ArenaMode::Off);
    }

    /// The two symbols codegen emits by name are the ones the runtime
    /// exports, with the signatures codegen declares — a rename or an arity
    /// change on either side would otherwise fail only at link time.
    #[test]
    fn emitted_symbols_name_the_runtime_exports() {
        let alloc: unsafe extern "C" fn(u64, u32) -> *mut core::ffi::c_void =
            tungsten_runtime::__tungsten_alloc;
        let init: extern "C" fn() = tungsten_runtime::__tungsten_arena_init;
        let _ = (alloc, init);
        assert_eq!(ALLOC_SYMBOL, "__tungsten_alloc");
        assert_eq!(ARENA_INIT_SYMBOL, "__tungsten_arena_init");
    }
}

/// ADR 18.9.26c AC 2: the two shapes `build_malloc_call` emits, read off the
/// printed IR of a one-allocation function.
#[cfg(test)]
mod alloc_shape_tests {
    use super::{AllocClass, CodeGen};
    use inkwell::context::Context;

    /// Emit one `Env`-class allocation in a fresh function and return the
    /// function's IR text (verified, so a malformed phi fails here).
    fn emit_one_allocation(alloc_profile: bool) -> String {
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "alloc_shape");
        codegen.tracing.alloc_profile = alloc_profile;
        let ptr = context.ptr_type(inkwell::AddressSpace::default());
        let function = codegen
            .module
            .add_function("probe", ptr.fn_type(&[], false), None);
        codegen
            .builder
            .position_at_end(context.append_basic_block(function, "entry"));
        let size = context.i64_type().const_int(24, false);
        let allocation = codegen
            .build_malloc_call(size, AllocClass::Env, "env")
            .expect("allocation builds");
        codegen
            .builder
            .build_return(Some(&allocation))
            .expect("return");
        assert!(function.verify(true), "probe must verify");
        codegen.module.print_to_string().to_string()
    }

    #[test]
    fn non_profiled_allocation_branches_on_the_mode_flag() {
        let ir = emit_one_allocation(false);
        assert!(ir.contains("load i32, ptr @__tungsten_arena_mode"), "{ir}");
        assert!(ir.contains("icmp eq i32 %arena.mode, 0"), "{ir}");
        assert!(
            ir.contains("br i1 %arena.off, label %alloc.malloc, label %alloc.arena"),
            "{ir}"
        );
        assert!(
            ir.contains("label %alloc.arena, !prof !0")
                && ir.contains("!0 = !{!\"branch_weights\", i32 2000, i32 1}"),
            "the off arm is weighted as the likely one: {ir}"
        );
        assert!(ir.contains("call ptr @malloc(i64 24)"), "{ir}");
        assert!(
            ir.contains("call ptr @__tungsten_alloc(i64 24, i32 1)"),
            "{ir}"
        );
        assert!(
            ir.contains("phi ptr [ %env.malloc, %alloc.malloc ], [ %env.arena, %alloc.arena ]"),
            "{ir}"
        );
        assert!(ir.contains("ret ptr %env"), "{ir}");
    }

    /// The `@malloc` call sits in the `alloc.malloc` block — the invariant
    /// `check-arena-mode` scans for.
    #[test]
    fn non_profiled_malloc_call_is_inside_the_malloc_block() {
        let ir = emit_one_allocation(false);
        let block = ir
            .split("\nalloc.malloc:")
            .nth(1)
            .and_then(|rest| rest.split("\nalloc.").next())
            .expect("an alloc.malloc block");
        assert!(block.contains("call ptr @malloc("), "{ir}");
        assert!(!block.contains("@__tungsten_alloc"), "{ir}");
    }

    #[test]
    fn profiled_allocation_always_calls_the_symbol() {
        let ir = emit_one_allocation(true);
        assert!(
            ir.contains("%env = call ptr @__tungsten_alloc(i64 24, i32 1)"),
            "{ir}"
        );
        assert!(!ir.contains("call ptr @malloc"), "{ir}");
        assert!(!ir.contains("load i32, ptr @__tungsten_arena_mode"), "{ir}");
        assert!(!ir.contains("phi"), "{ir}");
    }

    /// Two allocations in one function get distinct, `alloc.malloc`-prefixed
    /// blocks, and the fixture setups no longer double-declare `malloc`.
    #[test]
    fn repeated_allocations_uniquify_block_names_and_declare_malloc_once() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "alloc_twice");
        let ptr = context.ptr_type(inkwell::AddressSpace::default());
        let function = codegen
            .module
            .add_function("probe", ptr.fn_type(&[], false), None);
        codegen
            .builder
            .position_at_end(context.append_basic_block(function, "entry"));
        let size = context.i64_type().const_int(8, false);
        let _ = codegen
            .build_malloc_call(size, AllocClass::Mu, "a")
            .unwrap();
        let second_allocation = codegen
            .build_malloc_call(size, AllocClass::Str, "b")
            .unwrap();
        codegen
            .builder
            .build_return(Some(&second_allocation))
            .unwrap();
        assert!(function.verify(true));
        let ir = codegen.module.print_to_string().to_string();
        // LLVM's suffix is a per-function counter shared by every name, so
        // only the prefix is stable — which is all `check-arena-mode` matches.
        let malloc_labels: Vec<&str> = ir
            .lines()
            .filter(|l| l.starts_with("alloc.malloc"))
            .collect();
        assert_eq!(malloc_labels.len(), 2, "{ir}");
        assert_ne!(malloc_labels[0], malloc_labels[1], "{ir}");
        assert!(
            ir.contains("call ptr @__tungsten_alloc(i64 8, i32 3)"),
            "{ir}"
        );
        assert!(!ir.contains("malloc.1"), "{ir}");
        assert_eq!(ir.matches("declare ptr @malloc(").count(), 1, "{ir}");
    }
}
