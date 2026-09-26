//! Signed integer operations compilation (ADR 14.9.26c §2.3).
//!
//! `+ - *` and negation lower through the `llvm.*.with.overflow.i64`
//! intrinsics to a branch into a trap block; `/ %` trap on a zero divisor and
//! on `MIN / -1`; comparisons are the signed predicates. The trap block calls
//! [`INT_TRAP_SYMBOL`] with the same code table the evaluator reads
//! (`IntTrapKind::code`), so both paths print one line and `diff exec` can
//! classify a trap on both sides as parity.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue};
use inkwell::IntPredicate;
use tungsten_core::eval::IntTrapKind;
use tungsten_core::terms::IntBinOp;

/// The runtime symbol a trap block calls: `tg_int_trap(kind: i32) -> !`.
pub(crate) const INT_TRAP_SYMBOL: &str = "tg_int_trap";

/// The overflow intrinsic for one arithmetic operator; `None` for the rest.
fn overflow_intrinsic(op: IntBinOp) -> Option<&'static str> {
    match op {
        IntBinOp::Add => Some("llvm.sadd.with.overflow"),
        IntBinOp::Sub => Some("llvm.ssub.with.overflow"),
        IntBinOp::Mul => Some("llvm.smul.with.overflow"),
        _ => None,
    }
}

/// The signed predicate for one comparison operator; `None` for arithmetic.
fn signed_predicate(op: IntBinOp) -> Option<IntPredicate> {
    match op {
        IntBinOp::Eq => Some(IntPredicate::EQ),
        IntBinOp::Lt => Some(IntPredicate::SLT),
        IntBinOp::Le => Some(IntPredicate::SLE),
        IntBinOp::Gt => Some(IntPredicate::SGT),
        IntBinOp::Ge => Some(IntPredicate::SGE),
        _ => None,
    }
}

fn llvm_err(e: impl ToString) -> CodeGenError {
    CodeGenError::LlvmError(e.to_string())
}

impl<'ctx> CodeGen<'ctx> {
    /// Compile `a op b` for signed integers: a comparison, a checked
    /// `+ - *`, or a guarded `/ %`.
    pub(crate) fn compile_int_bin(
        &mut self,
        op: IntBinOp,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        if let Some(predicate) = signed_predicate(op) {
            let name = format!("int_{}", op_name(op));
            let result = self
                .builder
                .build_int_compare(predicate, a, b, &name)
                .map_err(llvm_err)?;
            return Ok(result.into());
        }
        if let Some(intrinsic) = overflow_intrinsic(op) {
            let value = self.compile_checked_int_op(intrinsic, a, b, IntTrapKind::Overflow(op))?;
            return Ok(value.into());
        }
        self.compile_int_div_or_mod(op, a, b)
    }

    /// Compile `−a`: `0 − a` through the checked subtraction, so `−MIN` traps.
    pub(crate) fn compile_int_neg(
        &mut self,
        a: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let zero = self.context.i64_type().const_zero();
        let value = self.compile_checked_int_op(
            "llvm.ssub.with.overflow",
            zero,
            a,
            IntTrapKind::NegationOverflow,
        )?;
        Ok(value.into())
    }

    /// Compile `to_int(n)`: the bits pass through; a value above the signed
    /// maximum traps. One unsigned compare and a branch.
    pub(crate) fn compile_nat_to_int(
        &mut self,
        n: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let max = self.context.i64_type().const_int(i64::MAX as u64, false);
        let above = self
            .builder
            .build_int_compare(IntPredicate::UGT, n, max, "nat_above_max")
            .map_err(llvm_err)?;
        self.emit_int_trap_branch(above, IntTrapKind::NatAboveSignedMax)?;
        Ok(n.into())
    }

    /// Compile `from_int(i)`: negatives clamp to 0. One `select`, no trap.
    pub(crate) fn compile_int_to_nat(
        &self,
        i: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let zero = self.context.i64_type().const_zero();
        let negative = self
            .builder
            .build_int_compare(IntPredicate::SLT, i, zero, "int_negative")
            .map_err(llvm_err)?;
        self.builder
            .build_select(negative, zero, i, "from_int")
            .map_err(llvm_err)
    }

    /// `/` and `%`: trap on a zero divisor, trap on `MIN / -1`, then the
    /// signed instruction.
    fn compile_int_div_or_mod(
        &mut self,
        op: IntBinOp,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        let i64_type = self.context.i64_type();
        let zero = i64_type.const_zero();
        let by_zero = self
            .builder
            .build_int_compare(IntPredicate::EQ, b, zero, "int_div_by_zero")
            .map_err(llvm_err)?;
        self.emit_int_trap_branch(by_zero, IntTrapKind::DivisionByZero)?;

        let min = i64_type.const_int(i64::MIN as u64, true);
        let neg_one = i64_type.const_int(-1i64 as u64, true);
        let is_min = self
            .builder
            .build_int_compare(IntPredicate::EQ, a, min, "int_is_min")
            .map_err(llvm_err)?;
        let is_neg_one = self
            .builder
            .build_int_compare(IntPredicate::EQ, b, neg_one, "int_is_neg_one")
            .map_err(llvm_err)?;
        let min_over_neg_one = self
            .builder
            .build_and(is_min, is_neg_one, "int_min_div_neg_one")
            .map_err(llvm_err)?;
        self.emit_int_trap_branch(min_over_neg_one, IntTrapKind::Overflow(op))?;

        let result = match op {
            IntBinOp::Div => self.builder.build_int_signed_div(a, b, "int_div"),
            IntBinOp::Mod => self.builder.build_int_signed_rem(a, b, "int_mod"),
            other => unreachable!("compile_int_div_or_mod called with {other:?}"),
        }
        .map_err(llvm_err)?;
        Ok(result.into())
    }

    /// Call an `llvm.<s?>.with.overflow.i64` intrinsic, branch on its overflow
    /// flag into a trap block, and yield the value in the continuation.
    fn compile_checked_int_op(
        &mut self,
        intrinsic_name: &str,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
        trap: IntTrapKind,
    ) -> Result<IntValue<'ctx>, CodeGenError> {
        let intrinsic = inkwell::intrinsics::Intrinsic::find(intrinsic_name).ok_or_else(|| {
            CodeGenError::LlvmError(format!("intrinsic '{intrinsic_name}' not found"))
        })?;
        let i64_type = self.context.i64_type();
        let decl = intrinsic
            .get_declaration(&self.module, &[i64_type.into()])
            .ok_or_else(|| {
                CodeGenError::LlvmError(format!("cannot declare intrinsic '{intrinsic_name}'"))
            })?;
        let call = self
            .builder
            .build_call(decl, &[a.into(), b.into()], "int_checked")
            .map_err(llvm_err)?;
        let pair = call
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodeGenError::LlvmError("overflow intrinsic returned void".into()))?
            .into_struct_value();
        let value = self
            .builder
            .build_extract_value(pair, 0, "int_value")
            .map_err(llvm_err)?
            .into_int_value();
        let overflowed = self
            .builder
            .build_extract_value(pair, 1, "int_overflowed")
            .map_err(llvm_err)?
            .into_int_value();
        self.emit_int_trap_branch(overflowed, trap)?;
        Ok(value)
    }

    /// `br i1 %cond, label %trap, label %cont`; the trap block calls
    /// `tg_int_trap(code)` and is `unreachable`; the builder is left at `cont`.
    fn emit_int_trap_branch(
        &mut self,
        condition: IntValue<'ctx>,
        trap: IntTrapKind,
    ) -> Result<(), CodeGenError> {
        let function = self
            .compilation
            .current_fn
            .ok_or_else(|| CodeGenError::LlvmError("no current function".to_string()))?;
        let trap_block = self.context.append_basic_block(function, "int_trap");
        let cont_block = self.context.append_basic_block(function, "int_cont");
        self.builder
            .build_conditional_branch(condition, trap_block, cont_block)
            .map_err(llvm_err)?;

        self.builder.position_at_end(trap_block);
        let trap_fn = self.int_trap_function();
        let code = self
            .context
            .i32_type()
            .const_int(u64::from(trap.code()), false);
        self.builder
            .build_call(trap_fn, &[code.into()], "")
            .map_err(llvm_err)?;
        self.builder.build_unreachable().map_err(llvm_err)?;

        self.builder.position_at_end(cont_block);
        Ok(())
    }

    /// `tg_int_trap(i32) -> void`, declared `noreturn`, once per module.
    fn int_trap_function(&self) -> FunctionValue<'ctx> {
        self.module
            .get_function(INT_TRAP_SYMBOL)
            .unwrap_or_else(|| {
                let fn_type = self
                    .context
                    .void_type()
                    .fn_type(&[self.context.i32_type().into()], false);
                let function = self.module.add_function(INT_TRAP_SYMBOL, fn_type, None);
                let noreturn = self
                    .context
                    .create_enum_attribute(Attribute::get_named_enum_kind_id("noreturn"), 0);
                function.add_attribute(AttributeLoc::Function, noreturn);
                function
            })
    }
}

/// The operator's name as it appears in the IR value names.
fn op_name(op: IntBinOp) -> &'static str {
    match op {
        IntBinOp::Add => "add",
        IntBinOp::Sub => "sub",
        IntBinOp::Mul => "mul",
        IntBinOp::Div => "div",
        IntBinOp::Mod => "mod",
        IntBinOp::Eq => "eq",
        IntBinOp::Lt => "lt",
        IntBinOp::Le => "le",
        IntBinOp::Gt => "gt",
        IntBinOp::Ge => "ge",
    }
}

#[cfg(test)]
#[path = "int_ops_tests.rs"]
mod tests;
