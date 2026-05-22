//! Term compilation dispatch.
//!
//! Routes each Term variant to the appropriate specialized compilation method.

use super::backend::CodeGenError;
use super::CodeGen;
use inkwell::values::BasicValueEnum;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

impl<'ctx> CodeGen<'ctx> {
    /// Compile a term to an LLVM value.
    ///
    /// This is the main dispatch function that routes to specialized
    /// compilation methods in submodules.
    ///
    /// Tail position tracking: saves `in_tail_position` and resets to `false`
    /// so sub-expressions default to non-tail. Terms that propagate tail
    /// position (App, Let, If, Match, Case, transparent wrappers) receive
    /// the saved flag and restore it for their tail-position sub-terms.
    pub fn compile_term(&mut self, term: &Term) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let is_tail = self.compilation.in_tail_position;
        self.compilation.in_tail_position = false;

        match term {
            // Variables
            Term::Var(x) => self.compile_var(x),

            // Lambda calculus (closures.rs)
            Term::Lambda(x, param_ty, body) => self.compile_lambda(x, param_ty, body),
            Term::App(_, _) => {
                // Try direct (uncurried) call first for saturated known-arity calls
                if let Some(result) = self.try_compile_direct_call(term, is_tail)? {
                    return Ok(result);
                }
                // Fall through to closure path
                if let Term::App(func, arg) = term {
                    self.compile_app(func, arg, is_tail)
                } else {
                    unreachable!()
                }
            }
            Term::Let(x, ty, def, body) => self.compile_let(x, ty, def, body, is_tail),

            // Primitives (primitives.rs)
            Term::True => Ok(self.compile_true()),
            Term::False => Ok(self.compile_false()),
            Term::Unit => Ok(self.compile_unit()),
            Term::Zero => Ok(self.compile_zero()),
            Term::Succ(n) => {
                let n_val = self.compile_term(n)?;
                self.compile_succ(n_val)
            }
            Term::NatLit(n) => Ok(self.compile_nat_lit(*n)),
            Term::Absurd(ty, _) => self.compile_absurd(ty),
            Term::Sorry => self.compile_sorry(),

            // Booleans (control.rs)
            Term::If(cond, then_, else_) => self.compile_if(cond, then_, else_, is_tail),

            // Natural number recursion (control.rs)
            Term::NatRec(result_ty, zero_case, succ_case, n)
            | Term::NatInd(result_ty, zero_case, succ_case, n) => {
                self.compile_natrec(result_ty, zero_case, succ_case, n)
            }

            // Strings (strings.rs)
            Term::StringLit(s) => self.compile_string_lit(s),
            Term::StrConcat(s1, s2) => self.compile_str_concat(s1, s2),
            Term::StrLen(s) => self.compile_str_len(s),
            Term::StrEq(s1, s2) => self.compile_str_eq(s1, s2),
            Term::StrCharAt(s, idx) => self.compile_str_char_at(s, idx),
            Term::StrSubstring(s, start, len) => self.compile_str_substring(s, start, len),

            // Products (products.rs)
            Term::Pair(t1, t2) => self.compile_pair(t1, t2),
            Term::Fst(t) => self.compile_fst(t),
            Term::Snd(t) => self.compile_snd(t),

            // Sums (sums.rs)
            Term::Inl(sum_ty, t) => self.compile_inl(sum_ty, t),
            Term::Inr(sum_ty, t) => self.compile_inr(sum_ty, t),
            Term::Case(scrut, x, left, y, right) => {
                use crate::codegen::data::sums::CaseBranch;
                let left_branch = CaseBranch { var: x, body: left };
                let right_branch = CaseBranch {
                    var: y,
                    body: right,
                };
                self.compile_case(scrut, &left_branch, &right_branch, is_tail)
            }

            // Polymorphism (polymorphism.rs)
            Term::TyAbs(_var, body) => {
                self.compilation.in_tail_position = is_tail;
                self.compile_term(body)
            }
            Term::TyApp(t, ty_arg) => self.compile_ty_app(t, ty_arg),

            // Equality (proof erasure)
            Term::Refl(_, _) => Ok(self.compile_unit()),
            Term::Subst(_, _, _, proof) => {
                self.compilation.in_tail_position = is_tail;
                self.compile_term(proof)
            }

            // Recursion (closures.rs)
            Term::Fix(f, ty, body) => self.compile_fix(f, ty, body),

            // Recursive types (sums.rs)
            Term::Fold(mu_ty, t) => self.compile_fold(mu_ty, t),
            Term::Unfold(mu_ty, t) => self.compile_unfold(mu_ty, t),

            // Meta
            Term::Annot(t, _) => {
                self.compilation.in_tail_position = is_tail;
                self.compile_term(t)
            }

            // Globals (globals.rs)
            Term::Global(name) => self.compile_global(name),

            // Nat binary ops (nat_ops.rs) — dispatched via helper
            Term::NatLt(a, b)
            | Term::NatLe(a, b)
            | Term::NatGt(a, b)
            | Term::NatGe(a, b)
            | Term::NatAdd(a, b)
            | Term::NatSub(a, b)
            | Term::NatMul(a, b)
            | Term::NatDiv(a, b)
            | Term::NatMod(a, b)
            | Term::NatEq(a, b) => self.compile_nat_binop_dispatch(term, a, b),

            // Boolean operations (bool_ops.rs)
            Term::BoolAnd(a, b) | Term::BoolOr(a, b) => {
                self.compile_bool_binop_dispatch(term, a, b)
            }
            Term::BoolNot(a) => {
                let a_val = self.compile_term(a)?.into_int_value();
                self.compile_bool_not(a_val)
            }

            // External calls (globals.rs)
            Term::ExternCall(symbol, args) => self.compile_extern_call_term(symbol, args, term),

            // References (refs.rs)
            Term::RefNew(val) => {
                let val_compiled = self.compile_term(val)?;
                self.compile_ref_new(val_compiled)
            }
            Term::RefGet(ref_term) => self.compile_ref_get_term(ref_term),
            Term::RefSet(ref_term, val) => {
                let ref_ptr = self.compile_term(ref_term)?.into_pointer_value();
                let val_compiled = self.compile_term(val)?;
                self.compile_ref_set(ref_ptr, val_compiled)
            }

            // ADT (adt.rs)
            Term::AdtConstruct(adt_ty, variant_idx, payload) => {
                self.compile_adt_construct(adt_ty, *variant_idx, payload)
            }
            Term::AdtMatch(scrutinee, arms) => self.compile_adt_match(scrutinee, arms, is_tail),

            // Span wrapper: update debug location, then compile inner term
            Term::Spanned(inner, span) => {
                self.set_debug_location_for_span(span);
                self.compilation.in_tail_position = is_tail;
                self.compile_term(inner)
            }

            // Early return (ADR 13.5.26d) — compile inner value and emit LLVM ret
            Term::Return(inner) => self.compile_return(inner),
        }
    }

    /// Dispatch a nat binary op by looking up the operation from the term variant.
    fn compile_nat_binop_dispatch(
        &mut self,
        term: &Term,
        a: &Term,
        b: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let a_val = self.compile_term(a)?.into_int_value();
        let b_val = self.compile_term(b)?.into_int_value();
        match term {
            Term::NatLt(..) => self.compile_nat_lt(a_val, b_val),
            Term::NatLe(..) => self.compile_nat_le(a_val, b_val),
            Term::NatGt(..) => self.compile_nat_gt(a_val, b_val),
            Term::NatGe(..) => self.compile_nat_ge(a_val, b_val),
            Term::NatAdd(..) => self.compile_nat_add(a_val, b_val),
            Term::NatSub(..) => self.compile_nat_sub(a_val, b_val),
            Term::NatMul(..) => self.compile_nat_mul(a_val, b_val),
            Term::NatDiv(..) => self.compile_nat_div(a_val, b_val),
            Term::NatMod(..) => self.compile_nat_mod(a_val, b_val),
            Term::NatEq(..) => self.compile_nat_eq(a_val, b_val),
            _ => unreachable!(),
        }
    }

    /// Dispatch a bool binary op.
    fn compile_bool_binop_dispatch(
        &mut self,
        term: &Term,
        a: &Term,
        b: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let a_val = self.compile_term(a)?.into_int_value();
        let b_val = self.compile_term(b)?.into_int_value();
        match term {
            Term::BoolAnd(..) => self.compile_bool_and(a_val, b_val),
            Term::BoolOr(..) => self.compile_bool_or(a_val, b_val),
            _ => unreachable!(),
        }
    }

    /// Compile a `StrLen` term.
    fn compile_str_len(&mut self, s: &Term) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let str_val = self.compile_term(s)?;
        let len = self
            .builder
            .build_extract_value(str_val.into_struct_value(), 1, "strlen")
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        Ok(len)
    }

    /// Compile a `RefGet` term (infers inner type, loads from pointer).
    fn compile_ref_get_term(
        &mut self,
        ref_term: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let ref_ptr = self.compile_term(ref_term)?.into_pointer_value();
        let ref_ty = self.infer_term_type(ref_term)?;
        let inner_ty = match ref_ty {
            Type::Ref(inner) => self.types.lower_type(&inner),
            _ => {
                return Err(CodeGenError::TypeError(
                    "ref_get on non-ref type".to_string(),
                ))
            }
        };
        self.compile_ref_get(ref_ptr, inner_ty)
    }

    /// Compile an `ExternCall` term.
    fn compile_extern_call_term(
        &mut self,
        symbol: &str,
        args: &[Term],
        term: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let compiled_args: Result<Vec<_>, _> =
            args.iter().map(|arg| self.compile_term(arg)).collect();
        let compiled_args = compiled_args?;
        let ret_type = self.infer_term_type(term)?;
        let ret_llvm = self.types.lower_type(&ret_type);
        let result = self.compile_extern_call(symbol, compiled_args, ret_llvm)?;

        // If return type is Never/Void, the function doesn't return.
        // Also check for tg_exit by name since Never gets encoded as Unit.
        // Emit unreachable and create dead block for any subsequent code.
        let is_noreturn = self.types.is_uninhabited_type(&ret_type) || symbol.contains("tg_exit");

        if is_noreturn {
            self.builder
                .build_unreachable()
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

            // Create a dead block for any subsequent code in the same function
            if let Some(function) = self.compilation.current_fn {
                let dead_bb = self.context.append_basic_block(function, "never_dead");
                self.builder.position_at_end(dead_bb);
            }
        }
        Ok(result)
    }

    /// Compile a let binding.
    fn compile_let(
        &mut self,
        x: &str,
        ty: &Type,
        def: &Term,
        body: &Term,
        is_tail: bool,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        // Track heap-origin + last-use for string concat optimization (ADR 19.5.26a).
        // Only insert when def is StrConcat AND body uses x exactly once.
        let is_str_concat_binding = matches!(def, Term::StrConcat(_, _));
        let inserted_last_use = is_str_concat_binding && body.var_use_count(x) == 1;
        if inserted_last_use {
            self.compilation.heap_origin_vars.insert(x.to_string());
            self.compilation.last_use_vars.insert(x.to_string());
        }

        // Set binding name so nested lambdas can use source-level names
        let prev_binding = self.naming.current_binding_name.take();
        self.naming.current_binding_name = Some(x.to_string());

        // def is NOT in tail position (in_tail_position already false)
        let def_val = self.compile_term(def)?;

        // Restore previous binding context
        self.naming.current_binding_name = prev_binding;

        let old = self
            .compilation
            .env
            .insert(x.to_string(), (def_val, ty.clone()));
        // body IS in tail position if the let is
        self.compilation.in_tail_position = is_tail;
        let result = self.compile_term(body)?;

        // Clean up last-use/heap-origin tracking (only if we inserted)
        if inserted_last_use {
            self.compilation.last_use_vars.remove(x);
            self.compilation.heap_origin_vars.remove(x);
        }

        if let Some(old_val) = old {
            self.compilation.env.insert(x.to_string(), old_val);
        } else {
            self.compilation.env.remove(x);
        }
        Ok(result)
    }

    /// Compile an early return: evaluate inner term, emit LLVM ret, create dead block.
    fn compile_return(&mut self, inner: &Term) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let result = self.compile_term(inner)?;

        // Cast to the function's expected return type if needed
        if let Some(function) = self.compilation.current_fn {
            if let Some(ret_ty) = function.get_type().get_return_type() {
                let result = self.cast_to_type(result, ret_ty)?;
                self.builder
                    .build_return(Some(&result))
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            } else {
                self.builder
                    .build_return(None)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }

            // Create dead block for any subsequent code
            let dead_bb = self.context.append_basic_block(function, "after_return");
            self.builder.position_at_end(dead_bb);
        }

        // Return unit as the "value" of the return expression (type is ⊥, never used)
        Ok(self.compile_unit())
    }
}
