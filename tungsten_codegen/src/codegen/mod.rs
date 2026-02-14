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

mod backend;
mod data;
mod error;
mod exec;

pub use error::CodeGenError;

use crate::types::TypeLowering;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{InitializationConfig, Target, TargetMachine};
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue};
use inkwell::AddressSpace;
use std::collections::HashMap;
use std::collections::HashSet;
use tungsten_core::terms::{Term, Var};
use tungsten_core::types::Type;

/// LLVM code generator for Tungsten.
pub struct CodeGen<'ctx> {
    pub(crate) context: &'ctx Context,
    pub(crate) module: Module<'ctx>,
    pub(crate) builder: Builder<'ctx>,
    pub(crate) types: TypeLowering<'ctx>,

    /// Current function being compiled.
    pub(crate) current_fn: Option<FunctionValue<'ctx>>,

    /// Variable bindings: name -> (value, type).
    pub(crate) env: HashMap<Var, (BasicValueEnum<'ctx>, Type)>,

    /// Top-level definition types: name -> type.
    pub(crate) def_types: HashMap<String, Type>,

    /// Extern name mappings: original_name -> llvm_name.
    /// Extern wrappers are renamed to avoid shadowing C runtime symbols.
    pub(crate) extern_name_map: HashMap<String, String>,

    /// Counter for generating unique names.
    pub(crate) counter: u64,

    /// Lambda counter for unique function names.
    pub(crate) lambda_counter: u64,

    /// Original term definitions for monomorphization.
    pub(crate) term_defs: HashMap<String, Term>,

    /// Already monomorphized function instances.
    pub(crate) monomorphized: HashMap<(String, String), String>,

    /// Functions currently being monomorphized (to prevent infinite recursion).
    pub(crate) monomorphizing: HashSet<(String, String)>,
}

impl<'ctx> CodeGen<'ctx> {
    /// Create a new code generator.
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
            current_fn: None,
            env: HashMap::new(),
            def_types: HashMap::new(),
            extern_name_map: HashMap::new(),
            counter: 0,
            lambda_counter: 0,
            term_defs: HashMap::new(),
            monomorphized: HashMap::new(),
            monomorphizing: HashSet::new(),
        };

        cg.declare_runtime_functions();
        cg
    }

    /// Register extern name mappings for Global lookups.
    pub fn register_extern_name_map(&mut self, map: HashMap<String, String>) {
        self.extern_name_map = map;
    }

    /// Register record types for expansion during type lowering.
    pub fn register_record_types(&mut self, records: HashMap<String, Vec<(String, Type)>>) {
        self.types.register_record_types(records);
    }

    /// Register ADT types for expansion during type lowering.
    pub fn register_adt_types(
        &mut self,
        adts: HashMap<String, (Vec<String>, Vec<crate::types::CodegenConstructor>)>,
    ) {
        self.types.register_adt_types(adts);
    }

    /// Register a term definition for potential monomorphization.
    pub fn register_term_def(&mut self, name: &str, term: Term) {
        self.term_defs.insert(name.to_string(), term);
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
    }

    /// Generate a unique name.
    pub(crate) fn fresh_name(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{}_{}", prefix, self.counter)
    }

    /// Generate a unique lambda function name.
    pub(crate) fn fresh_lambda_name(&mut self) -> String {
        self.lambda_counter += 1;
        format!("__lambda_{}", self.lambda_counter)
    }

    /// Get the LLVM module.
    pub fn module(&self) -> &Module<'ctx> {
        &self.module
    }

    /// Declare a top-level definition (add function signature to module).
    pub fn declare_def(
        &mut self,
        name: &str,
        ty: &Type,
    ) -> Result<FunctionValue<'ctx>, CodeGenError> {
        self.def_types.insert(name.to_string(), ty.clone());

        let env_ptr_type = self.context.ptr_type(AddressSpace::default());

        let fn_type = match ty {
            Type::Arrow(param_ty, ret_ty) => {
                let param = self.types.lower_type(param_ty);
                let ret = self.types.lower_type(ret_ty);
                ret.fn_type(&[env_ptr_type.into(), param.into()], false)
            }
            _ => {
                let ret = self.types.lower_type(ty);
                ret.fn_type(&[env_ptr_type.into()], false)
            }
        };

        let function = self.module.add_function(name, fn_type, None);
        Ok(function)
    }

    /// Compile a top-level definition.
    pub fn compile_def(
        &mut self,
        name: &str,
        term: &Term,
        ty: &Type,
    ) -> Result<FunctionValue<'ctx>, CodeGenError> {
        self.def_types.insert(name.to_string(), ty.clone());

        let function = self.module.get_function(name).ok_or_else(|| {
            CodeGenError::Unsupported(format!(
                "function '{}' not declared (call declare_def first)",
                name
            ))
        })?;
        self.current_fn = Some(function);

        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);
        self.env.clear();

        if let Type::Arrow(param_ty, ret_ty) = ty {
            if let Term::Lambda(x, _, body) = term {
                let param_value = function.get_nth_param(1).ok_or_else(|| {
                    CodeGenError::TypeError("expected parameter for function".to_string())
                })?;
                self.env
                    .insert(x.clone(), (param_value, param_ty.as_ref().clone()));

                let expected_ret_ty = self.types.lower_type(ret_ty);
                let result = self.compile_term(body)?;
                let result = self.cast_to_type(result, expected_ret_ty)?;

                // Don't emit return if the current block already has a terminator
                // (e.g., unreachable from a noreturn call)
                let current_bb = self.builder.get_insert_block().unwrap();
                if current_bb.get_terminator().is_none() {
                    self.builder
                        .build_return(Some(&result))
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                }
            } else {
                return Err(CodeGenError::TypeError(
                    "expected lambda for function type".to_string(),
                ));
            }
        } else {
            let expected_ty = self.types.lower_type(ty);
            let result = self.compile_term(term)?;
            let result = self.cast_to_type(result, expected_ty)?;

            // Don't emit return if the current block already has a terminator
            let current_bb = self.builder.get_insert_block().unwrap();
            if current_bb.get_terminator().is_none() {
                self.builder
                    .build_return(Some(&result))
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }
        }

        // Verify module after each top-level function
        if self.monomorphizing.is_empty() {
            if let Err(e) = self.module.verify() {
                eprintln!(
                    "LLVM verification failed after compiling '{}': {}",
                    name,
                    e.to_string()
                );
                return Err(CodeGenError::LlvmError(format!(
                    "Module verification failed: {}",
                    e.to_string()
                )));
            }
        }

        Ok(function)
    }

    /// Compile a main function wrapper.
    pub fn compile_main_wrapper(
        &mut self,
        main_ty: &Type,
    ) -> Result<FunctionValue<'ctx>, CodeGenError> {
        let i32_type = self.context.i32_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let main_type = i32_type.fn_type(&[i32_type.into(), ptr_type.into()], false);
        let c_main = self.module.add_function("main", main_type, None);

        let entry = self.context.append_basic_block(c_main, "entry");
        self.builder.position_at_end(entry);

        let argc = c_main.get_nth_param(0).unwrap();
        let argv = c_main.get_nth_param(1).unwrap();

        if self.module.get_function("tg_init_args").is_some() {
            let init_args_c_type = self
                .context
                .void_type()
                .fn_type(&[i32_type.into(), ptr_type.into()], false);
            let init_args_c = self
                .module
                .add_function("tg_init_args_c", init_args_c_type, None);

            self.builder
                .build_call(init_args_c, &[argc.into(), argv.into()], "")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }

        if let Some(tungsten_main) = self.module.get_function("tungsten_main") {
            let null_env = ptr_type.const_null();
            let result = self
                .builder
                .build_call(tungsten_main, &[null_env.into()], "main_result")
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?
                .try_as_basic_value()
                .left();

            self.print_value(result, main_ty)?;
        }

        let zero = i32_type.const_int(0, false);
        self.builder
            .build_return(Some(&zero))
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

        Ok(c_main)
    }

    /// Print a value based on its type.
    fn print_value(
        &mut self,
        value: Option<BasicValueEnum<'ctx>>,
        ty: &Type,
    ) -> Result<(), CodeGenError> {
        let printf = self
            .module
            .get_function("printf")
            .ok_or_else(|| CodeGenError::LlvmError("printf not declared".to_string()))?;

        match ty {
            Type::Nat => {
                let fmt = self
                    .builder
                    .build_global_string_ptr("%lld\n", "nat_fmt")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                if let Some(v) = value {
                    self.builder
                        .build_call(printf, &[fmt.as_pointer_value().into(), v.into()], "")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                }
            }
            Type::Bool => {
                let true_str = self
                    .builder
                    .build_global_string_ptr("true\n", "true_str")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                let false_str = self
                    .builder
                    .build_global_string_ptr("false\n", "false_str")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                if let Some(BasicValueEnum::IntValue(b)) = value {
                    let str_ptr = self
                        .builder
                        .build_select(
                            b,
                            true_str.as_pointer_value(),
                            false_str.as_pointer_value(),
                            "bool_str",
                        )
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                    self.builder
                        .build_call(printf, &[str_ptr.into()], "")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                }
            }
            Type::String => {
                let fmt = self
                    .builder
                    .build_global_string_ptr("%s\n", "str_fmt")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                if let Some(BasicValueEnum::StructValue(s)) = value {
                    let ptr = self
                        .builder
                        .build_extract_value(s, 0, "str_ptr")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                    self.builder
                        .build_call(printf, &[fmt.as_pointer_value().into(), ptr.into()], "")
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                }
            }
            Type::Unit => {
                let fmt = self
                    .builder
                    .build_global_string_ptr("()\n", "unit_fmt")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                self.builder
                    .build_call(printf, &[fmt.as_pointer_value().into()], "")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }
            _ => {
                let fmt = self
                    .builder
                    .build_global_string_ptr("<value>\n", "value_fmt")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                self.builder
                    .build_call(printf, &[fmt.as_pointer_value().into()], "")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Compile a term to an LLVM value.
    ///
    /// This is the main dispatch function that routes to specialized
    /// compilation methods in submodules.
    pub fn compile_term(&mut self, term: &Term) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        match term {
            // Variables
            Term::Var(x) => self.compile_var(x),

            // Lambda calculus (closures.rs)
            Term::Lambda(x, param_ty, body) => self.compile_lambda(x, param_ty, body),
            Term::App(func, arg) => self.compile_app(func, arg),
            Term::Let(x, ty, def, body) => self.compile_let(x, ty, def, body),

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
            Term::If(cond, then_, else_) => self.compile_if(cond, then_, else_),

            // Natural number recursion (control.rs)
            Term::NatRec(result_ty, zero_case, succ_case, n) => {
                self.compile_natrec(result_ty, zero_case, succ_case, n)
            }
            Term::NatInd(motive, zero_case, succ_case, n) => {
                self.compile_natrec(motive, zero_case, succ_case, n)
            }

            // Strings (strings.rs)
            Term::StringLit(s) => self.compile_string_lit(s),
            Term::StrConcat(s1, s2) => self.compile_str_concat(s1, s2),
            Term::StrLen(s) => {
                let str_val = self.compile_term(s)?;
                let len = self
                    .builder
                    .build_extract_value(str_val.into_struct_value(), 1, "strlen")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                Ok(len)
            }
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
            Term::Case(scrut, x, left, y, right) => self.compile_case(scrut, x, left, y, right),

            // Polymorphism (polymorphism.rs)
            Term::TyAbs(_var, body) => self.compile_term(body),
            Term::TyApp(t, ty_arg) => self.compile_ty_app(t, ty_arg),

            // Equality (proof erasure)
            Term::Refl(_, _) => Ok(self.compile_unit()),
            Term::Subst(_, _, _, proof) => self.compile_term(proof),

            // Recursion (closures.rs)
            Term::Fix(f, ty, body) => self.compile_fix(f, ty, body),

            // Recursive types (sums.rs)
            Term::Fold(mu_ty, t) => self.compile_fold(mu_ty, t),
            Term::Unfold(mu_ty, t) => self.compile_unfold(mu_ty, t),

            // Meta
            Term::Annot(t, _) => self.compile_term(t),

            // Globals (globals.rs)
            Term::Global(name) => self.compile_global(name),

            // Natural operations (nat_ops.rs)
            Term::NatLt(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_nat_lt(a_val, b_val)
            }
            Term::NatLe(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_nat_le(a_val, b_val)
            }
            Term::NatGt(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_nat_gt(a_val, b_val)
            }
            Term::NatGe(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_nat_ge(a_val, b_val)
            }
            Term::NatAdd(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_nat_add(a_val, b_val)
            }
            Term::NatSub(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_nat_sub(a_val, b_val)
            }
            Term::NatMul(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_nat_mul(a_val, b_val)
            }
            Term::NatDiv(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_nat_div(a_val, b_val)
            }
            Term::NatMod(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_nat_mod(a_val, b_val)
            }
            Term::NatEq(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_nat_eq(a_val, b_val)
            }

            // Boolean operations (bool_ops.rs)
            Term::BoolAnd(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_bool_and(a_val, b_val)
            }
            Term::BoolOr(a, b) => {
                let a_val = self.compile_term(a)?.into_int_value();
                let b_val = self.compile_term(b)?.into_int_value();
                self.compile_bool_or(a_val, b_val)
            }
            Term::BoolNot(a) => {
                let a_val = self.compile_term(a)?.into_int_value();
                self.compile_bool_not(a_val)
            }

            // External calls (globals.rs)
            Term::ExternCall(symbol, args) => {
                let compiled_args: Result<Vec<_>, _> =
                    args.iter().map(|arg| self.compile_term(arg)).collect();
                let compiled_args = compiled_args?;
                let ret_type = self.infer_term_type(term)?;
                let ret_llvm = self.types.lower_type(&ret_type);
                let result = self.compile_extern_call(symbol, compiled_args, ret_llvm)?;

                // If return type is Never/Void, the function doesn't return.
                // Also check for tg_exit by name since Never gets encoded as Unit.
                // Emit unreachable and create dead block for any subsequent code.
                let is_noreturn =
                    self.types.is_uninhabited_type(&ret_type) || symbol.contains("tg_exit");

                if is_noreturn {
                    self.builder
                        .build_unreachable()
                        .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;

                    // Create a dead block for any subsequent code in the same function
                    if let Some(function) = self.current_fn {
                        let dead_bb = self.context.append_basic_block(function, "never_dead");
                        self.builder.position_at_end(dead_bb);
                    }
                }
                Ok(result)
            }

            // References (refs.rs)
            Term::RefNew(val) => {
                let val_compiled = self.compile_term(val)?;
                self.compile_ref_new(val_compiled)
            }
            Term::RefGet(ref_term) => {
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
            Term::RefSet(ref_term, val) => {
                let ref_ptr = self.compile_term(ref_term)?.into_pointer_value();
                let val_compiled = self.compile_term(val)?;
                self.compile_ref_set(ref_ptr, val_compiled)
            }

            // ADT (adt.rs)
            Term::AdtConstruct(adt_ty, variant_idx, payload) => {
                self.compile_adt_construct(adt_ty, *variant_idx, payload)
            }
            Term::AdtMatch(scrutinee, arms) => self.compile_adt_match(scrutinee, arms),
        }
    }

    /// Compile a let binding.
    fn compile_let(
        &mut self,
        x: &str,
        ty: &Type,
        def: &Term,
        body: &Term,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let def_val = self.compile_term(def)?;
        let old = self.env.insert(x.to_string(), (def_val, ty.clone()));
        let result = self.compile_term(body)?;
        if let Some(old_val) = old {
            self.env.insert(x.to_string(), old_val);
        } else {
            self.env.remove(x);
        }
        Ok(result)
    }

    /// Get size of type in bytes using LLVM TargetData for accurate alignment.
    /// Delegates to TypeLowering::type_size() which holds the TargetData.
    pub(crate) fn type_size_bytes(&self, ty: BasicTypeEnum<'ctx>) -> u64 {
        self.types.type_size(ty)
    }

    /// Materialize a struct value through memory if it's large.
    ///
    /// On ARM64, structs larger than 16 bytes are returned via sret.
    pub(crate) fn materialize_call_result(
        &self,
        value: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        if let BasicValueEnum::StructValue(sv) = value {
            let ty = sv.get_type();
            let size = self.type_size_bytes(ty.into());

            if size > 16 {
                let alloca = self
                    .builder
                    .build_alloca(ty, "call_result_alloca")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                if let Some(inst) = alloca.as_instruction() {
                    let _ = inst.set_alignment(16);
                }
                let store = self
                    .builder
                    .build_store(alloca, sv)
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                let _ = store.set_alignment(16);
                let reloaded = self
                    .builder
                    .build_load(ty, alloca, "call_result_loaded")
                    .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
                if let Some(inst) = reloaded.as_instruction_value() {
                    let _ = inst.set_alignment(16);
                }
                return Ok(reloaded);
            }
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    #[test]
    fn test_codegen_new() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test");
        assert!(codegen.module.get_function("printf").is_some());
        assert!(codegen.module.get_function("malloc").is_some());
    }

    #[test]
    fn test_fresh_name() {
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "test");
        let name1 = codegen.fresh_name("var");
        let name2 = codegen.fresh_name("var");
        assert_ne!(name1, name2);
        assert!(name1.starts_with("var_"));
        assert!(name2.starts_with("var_"));
    }

    #[test]
    fn test_fresh_lambda_name() {
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "test");
        let name1 = codegen.fresh_lambda_name();
        let name2 = codegen.fresh_lambda_name();
        assert_ne!(name1, name2);
        assert!(name1.starts_with("__lambda_"));
    }

    #[test]
    fn test_register_extern_name_map() {
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "test");
        let mut map = HashMap::new();
        map.insert("original".to_string(), "remapped".to_string());
        codegen.register_extern_name_map(map);
        assert_eq!(
            codegen.extern_name_map.get("original"),
            Some(&"remapped".to_string())
        );
    }

    #[test]
    fn test_type_size_bytes() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test");

        let i8_ty = context.i8_type().into();
        assert_eq!(codegen.type_size_bytes(i8_ty), 1);

        let i64_ty = context.i64_type().into();
        assert_eq!(codegen.type_size_bytes(i64_ty), 8);

        let ptr_ty = context.ptr_type(AddressSpace::default()).into();
        assert_eq!(codegen.type_size_bytes(ptr_ty), 8);
    }

    #[test]
    fn test_type_size_bytes_struct_alignment() {
        // Test that struct sizes account for LLVM alignment padding
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test");

        let i8_ty = context.i8_type();
        let i64_ty = context.i64_type();
        let ptr_ty = context.ptr_type(AddressSpace::default());

        // Simple struct: { i64, i64 } should be 16 bytes
        let simple_struct = context.struct_type(&[i64_ty.into(), i64_ty.into()], false);
        let simple_size = codegen.type_size_bytes(simple_struct.into());
        assert!(
            simple_size >= 16,
            "{{ i64, i64 }} should be >= 16 bytes, got {}",
            simple_size
        );

        // Struct with padding: { i8, i64 } - i8 needs padding before i64
        let padded_struct = context.struct_type(&[i8_ty.into(), i64_ty.into()], false);
        let padded_size = codegen.type_size_bytes(padded_struct.into());
        // LLVM aligns i64 to 8 bytes, so i8 + 7 padding + i64 = 16
        assert!(
            padded_size >= 16,
            "{{ i8, i64 }} should be >= 16 bytes (alignment), got {}",
            padded_size
        );

        // Nested struct simulating Token × ptr (like List<Token> Cons payload)
        // TokenKind ≈ { i32, [64 x i8] }
        let payload_array = context.i8_type().array_type(64);
        let token_kind_ty =
            context.struct_type(&[context.i32_type().into(), payload_array.into()], false);
        // Span ≈ { i64, i64, { ptr, i64 } }
        let string_ty = context.struct_type(&[ptr_ty.into(), i64_ty.into()], false);
        let span_ty = context.struct_type(&[i64_ty.into(), i64_ty.into(), string_ty.into()], false);
        // Token = { TokenKind, Span }
        let token_ty = context.struct_type(&[token_kind_ty.into(), span_ty.into()], false);
        // Cons payload = { Token, ptr }
        let cons_payload = context.struct_type(&[token_ty.into(), ptr_ty.into()], false);

        let cons_size = codegen.type_size_bytes(cons_payload.into());
        // Must be large enough to hold the actual LLVM layout
        assert!(
            cons_size >= 100,
            "Cons<Token> payload should be >= 100 bytes, got {}",
            cons_size
        );
    }
}
