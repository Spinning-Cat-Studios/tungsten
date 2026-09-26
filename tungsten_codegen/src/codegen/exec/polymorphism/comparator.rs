//! Codegen-time resolution of the structural-comparator intrinsic
//! (ADR 29.6.26f P6′ step 2).
//!
//! `compare(a, b)` lowers (via the elaborator) to
//! `App(App(TyApp(Global("__cmp"), T), a), b)`. During monomorphization the outer
//! generic (`assert_eq<T>` etc.) is specialized, so by the time codegen reaches the
//! `TyApp(Global("__cmp"), …)` node its type argument `T` has been substituted to a
//! **concrete** type. This module resolves that node: it synthesizes the
//! `compare_T` closure on demand (via the `bootstrap`-supplied callback), emits the
//! transitively-referenced comparator functions once per unit (with internal
//! linkage, so per-unit copies never collide at link time), and returns the top
//! comparator as a callable closure.
//!
//! This mirrors the evaluator's lazy `__cmp<T>` resolution
//! (`tungsten_core::eval::env::handlers::types_and_recursion`), keeping the codegen
//! and eval paths structurally identical.

use crate::codegen::backend::CodeGenError;
use crate::codegen::exec::direct_calls::direct_name;
use crate::codegen::CodeGen;
use inkwell::module::Linkage;
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;
use std::rc::Rc;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

/// Callback that lazily synthesizes a structural comparator for a concrete type
/// (ADR 29.6.26f P6′ step 2). Given a concrete `T`, returns `Ok((top_symbol, defs))`
/// where `top_symbol` is the comparator to call (e.g. `compare_Nat`) and `defs`
/// is the transitive closure of `(name, term, type)` comparator definitions to
/// emit. Returns `Err(path)` if `T` is not comparable (the P3 rejection point on
/// the codegen path), where `path` locates the first incomparable field (e.g.
/// `"$.env: EvalEnv is opaque"`). `tungsten_codegen` cannot depend on `bootstrap`
/// (where synthesis lives), so the callback is supplied by `bootstrap` and installed
/// on `CodeGen` — mirroring the evaluator's `ComparatorSynth` (`tungsten_core::eval`).
pub type ComparatorSynth = Rc<dyn Fn(&Type) -> Result<(String, Vec<(String, Term, Type)>), String>>;

impl<'ctx> CodeGen<'ctx> {
    /// Resolve `TyApp(Global("__cmp"), ty_arg)` — the comparator intrinsic.
    ///
    /// Returns the `compare_T` function wrapped as a closure. Emits the comparator
    /// (and its transitive sub-comparators) into this unit on first use.
    pub(crate) fn compile_comparator_intrinsic(
        &mut self,
        ty_arg: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Still abstract? We are inside an un-monomorphized polymorphic body; the
        // outer `TyAbs` will be monomorphized first, at which point `ty_arg` becomes
        // concrete and we are re-entered. Return a zeroed closure placeholder — it
        // is never executed at runtime (matches `compile_ty_app_global`).
        if self.has_mono_blocking_tyvar(ty_arg) {
            return Ok(self.zeroed_closure_placeholder());
        }

        let synth = self.monomorph.comparator_synth.clone().ok_or_else(|| {
            CodeGenError::Unsupported(format!(
                "comparator intrinsic __cmp<{ty_arg:?}> reached codegen but no comparator \
                 synthesis callback is installed (ADR 29.6.26f P6′ step 2)"
            ))
        })?;

        // P3 rejection point on the codegen path: an incomparable type yields the
        // path to the first incomparable field (e.g. `$.env: EvalEnv is opaque`).
        let (top_symbol, defs) = synth(ty_arg).map_err(|path| {
            CodeGenError::Unsupported(format!(
                "type `{ty_arg:?}` is not comparable at `{path}` — no structural comparator \
                 can be synthesized (ADR 29.6.26f P3, `Comparable<T>`). Comparable leaves are \
                 primitives, strings, tuples, sums/ADTs, records, and lists thereof; closures \
                 and opaque handles are not."
            ))
        })?;

        self.emit_comparator_defs(&defs)?;

        let func = self.module.get_function(&top_symbol).ok_or_else(|| {
            CodeGenError::LlvmError(format!(
                "comparator `{top_symbol}` was not emitted despite successful synthesis"
            ))
        })?;
        self.wrap_function_as_closure(func)
    }

    /// Emit each synthesized comparator def exactly once per unit.
    ///
    /// All members are **declared** before any is **defined**, so mutually- and
    /// self-recursive comparators can reference each other's symbols (they call
    /// sub-comparators via `Global("compare_…")`). Per-unit copies use internal
    /// linkage to avoid cross-unit symbol collisions.
    ///
    /// Emission happens *mid-expression* (inside another function's open block), so
    /// a sentinel is pushed into `monomorph.in_progress` for the duration: this
    /// suppresses the per-def module verification (`verify_after_compile`), which
    /// would otherwise reject the still-incomplete enclosing function — the exact
    /// protocol `compile_monomorphized` uses for nested instances.
    fn emit_comparator_defs(&mut self, defs: &[(String, Term, Type)]) -> Result<(), CodeGenError> {
        let guard = ("__cmp".to_string(), "<emitting>".to_string());
        let outermost = self.monomorph.in_progress.insert(guard.clone());

        let result = self.emit_comparator_defs_inner(defs);

        // Only the outermost comparator emission removes the sentinel, so nested
        // emissions (a comparator body referencing another) stay guarded until the
        // whole cluster is complete.
        if outermost {
            self.monomorph.in_progress.remove(&guard);
        }
        result
    }

    fn emit_comparator_defs_inner(
        &mut self,
        defs: &[(String, Term, Type)],
    ) -> Result<(), CodeGenError> {
        // Declare-all-first (forward declarations for recursion).
        for (name, _term, ty) in defs {
            if self.monomorph.emitted_comparators.contains(name) {
                continue;
            }
            if self.module.get_function(name).is_none() {
                let func = self.declare_def(name, ty)?;
                func.set_linkage(Linkage::Internal);
                if let Some(direct) = self.module.get_function(&direct_name(name)) {
                    direct.set_linkage(Linkage::Internal);
                }
            }
        }
        // Define-all (each member once per unit).
        for (name, term, ty) in defs {
            if !self.monomorph.emitted_comparators.insert(name.clone()) {
                continue;
            }
            self.dump_synthesized_def(name, term, ty);
            self.compile_def_saved(name, term, ty)?;
        }
        Ok(())
    }

    /// Print a synthesized comparator's Core term when `--dump-synthesized` is
    /// active and its filter matches (ADR 12.7.26c P6). These terms are built by
    /// the P6′ intercept *after* elaboration, so `--dump-ir` cannot see them.
    fn dump_synthesized_def(&self, name: &str, term: &Term, ty: &Type) {
        let Some(filter) = &self.tracing.dump_synthesized else {
            return;
        };
        if let Some(sym) = filter {
            if !name.contains(sym.as_str()) {
                return;
            }
        }
        println!("── synthesized comparator `{name}` : {ty:?}");
        println!("{term:?}");
    }

    /// Compile a top-level def while preserving the caller's compilation state.
    ///
    /// Emitting a comparator happens *mid-expression* (we are inside some other
    /// function's body), so the builder position, env, current function, type
    /// substitution, and binding name must all be saved and restored — the same
    /// dance `compile_with_type_subst` performs for monomorphized instances. No
    /// type substitution is pushed: synthesized comparator terms are already
    /// concrete/monomorphic.
    fn compile_def_saved(
        &mut self,
        name: &str,
        term: &Term,
        ty: &Type,
    ) -> Result<(), CodeGenError> {
        let saved_block = self.builder.get_insert_block();
        let saved_env = self.compilation.env.clone();
        let saved_current_fn = self.compilation.current_fn;
        let saved_type_subst = self.types.type_subst().clone();
        let saved_binding = self.naming.current_binding_name.clone();
        // Enter synthesized-naming mode so this def's lambdas emit as
        // `<name>$l<i>` (ADR 12.7.26c P6/D7). Saved/restored to support nested
        // comparator emission.
        let saved_synth_binding = self.naming.synthesized_def_binding.take();
        let saved_synth_index = self.naming.synthesized_lambda_index;
        self.naming.synthesized_def_binding = Some(name.to_string());
        self.naming.synthesized_lambda_index = 0;

        let result = self.compile_def(name, term, ty);

        self.types.restore_type_subst(saved_type_subst);
        self.compilation.env = saved_env;
        self.compilation.current_fn = saved_current_fn;
        self.naming.current_binding_name = saved_binding;
        self.naming.synthesized_def_binding = saved_synth_binding;
        self.naming.synthesized_lambda_index = saved_synth_index;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
        result.map(|_| ())
    }

    /// A zeroed `{ptr, ptr}` closure value (placeholder for abstract type args).
    fn zeroed_closure_placeholder(&self) -> BasicValueEnum<'ctx> {
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let closure_type = self
            .context
            .struct_type(&[ptr_type.into(), ptr_type.into()], false);
        closure_type.const_zero().into()
    }
}
