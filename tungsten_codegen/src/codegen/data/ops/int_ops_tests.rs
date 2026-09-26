//! Tests for `int_ops.rs` (ADR 14.9.26c AC 3): the IR shape of every signed
//! operation — intrinsic, predicate, trap code — read back from the module
//! text, so a swapped operator or a dropped guard is a failed assertion, not
//! a runtime surprise. A `#[path]` sibling: the emitter alone sits near the
//! file cap.

use super::*;
use inkwell::context::Context;

/// A function with two `i64` parameters. The operands under test are those
/// parameters, never constants: inkwell's builder constant-folds `sdiv i64 7,
/// 2` to `3` at emission, and a folded instruction leaves no text to assert on.
fn setup_codegen_with_function(context: &Context) -> CodeGen {
    let mut codegen = CodeGen::new(context, "test");
    let i64_type = context.i64_type();
    let fn_type = context
        .void_type()
        .fn_type(&[i64_type.into(), i64_type.into()], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);
    codegen
}

/// The two parameters of the fixture function, as runtime `i64` operands.
fn operands<'ctx>(codegen: &CodeGen<'ctx>) -> (IntValue<'ctx>, IntValue<'ctx>) {
    let function = codegen.compilation.current_fn.expect("fixture function");
    let a = function.get_nth_param(0).expect("a").into_int_value();
    let b = function.get_nth_param(1).expect("b").into_int_value();
    (a, b)
}

fn ir(codegen: &CodeGen) -> String {
    codegen.module.print_to_string().to_string()
}

/// 14.9.26c AC 3: the IR carries the overflow intrinsic AND the trap symbol.
#[test]
fn checked_add_emits_the_overflow_intrinsic_and_the_trap_call() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);
    let (a, b) = operands(&codegen);

    let result = codegen.compile_int_bin(IntBinOp::Add, a, b).unwrap();
    assert_eq!(result.into_int_value().get_type().get_bit_width(), 64);

    let text = ir(&codegen);
    assert!(text.contains("llvm.sadd.with.overflow.i64"), "{text}");
    assert!(text.contains("call void @tg_int_trap(i32 0)"), "{text}");
    assert!(text.contains("unreachable"), "{text}");
    assert!(text.contains("noreturn"), "{text}");
}

/// Each arithmetic operator picks ITS intrinsic and ITS trap code — a
/// mutant that swapped two would still emit "an" intrinsic.
#[test]
fn each_checked_operator_has_its_own_intrinsic_and_code() {
    for (op, intrinsic, code) in [
        (IntBinOp::Sub, "llvm.ssub.with.overflow.i64", 1),
        (IntBinOp::Mul, "llvm.smul.with.overflow.i64", 2),
    ] {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);
        let (a, b) = operands(&codegen);
        codegen.compile_int_bin(op, a, b).unwrap();
        let text = ir(&codegen);
        assert!(text.contains(intrinsic), "{op:?}: {text}");
        assert!(
            text.contains(&format!("@tg_int_trap(i32 {code})")),
            "{op:?}: {text}"
        );
    }
}

/// Division guards BOTH traps, in the order the evaluator tests them
/// (zero first), then divides SIGNED.
#[test]
fn division_guards_zero_then_min_over_minus_one_then_sdiv() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);
    let (a, b) = operands(&codegen);
    codegen.compile_int_bin(IntBinOp::Div, a, b).unwrap();
    let text = ir(&codegen);
    let zero_trap = text.find("@tg_int_trap(i32 6)").expect("zero-divisor trap");
    let overflow_trap = text.find("@tg_int_trap(i32 3)").expect("MIN / -1 trap");
    assert!(zero_trap < overflow_trap, "{text}");
    assert!(text.contains("sdiv"), "{text}");
    assert!(!text.contains("udiv"), "{text}");
}

#[test]
fn modulo_uses_srem_and_its_own_overflow_code() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);
    let (a, b) = operands(&codegen);
    codegen.compile_int_bin(IntBinOp::Mod, a, b).unwrap();
    let text = ir(&codegen);
    assert!(text.contains("srem"), "{text}");
    assert!(text.contains("@tg_int_trap(i32 4)"), "{text}");
}

/// Comparisons are the SIGNED predicates and carry no trap.
#[test]
fn comparisons_are_signed_and_never_trap() {
    for (op, predicate, name) in [
        (IntBinOp::Lt, "icmp slt", "lt"),
        (IntBinOp::Le, "icmp sle", "le"),
        (IntBinOp::Gt, "icmp sgt", "gt"),
        (IntBinOp::Ge, "icmp sge", "ge"),
        (IntBinOp::Eq, "icmp eq", "eq"),
    ] {
        let context = Context::create();
        let mut codegen = setup_codegen_with_function(&context);
        let (a, b) = operands(&codegen);
        let result = codegen.compile_int_bin(op, a, b).unwrap();
        assert_eq!(result.into_int_value().get_type().get_bit_width(), 1);
        let text = ir(&codegen);
        // The value is named after ITS operator, so a swapped `op_name`
        // shows in the IR text a reader greps.
        assert!(
            text.contains(&format!("%int_{name} = {predicate}")),
            "{op:?}: {text}"
        );
        assert!(!text.contains("tg_int_trap"), "{op:?}: {text}");
    }
}

#[test]
fn negation_is_checked_subtraction_from_zero() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);
    let (a, _) = operands(&codegen);
    codegen.compile_int_neg(a).unwrap();
    let text = ir(&codegen);
    assert!(text.contains("llvm.ssub.with.overflow.i64"), "{text}");
    assert!(text.contains("@tg_int_trap(i32 5)"), "{text}");
}

#[test]
fn bridges_are_one_compare_and_branch_and_one_select() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);
    let (n, _) = operands(&codegen);
    codegen.compile_nat_to_int(n).unwrap();
    codegen.compile_int_to_nat(n).unwrap();
    let text = ir(&codegen);
    assert!(text.contains("icmp ugt"), "{text}");
    assert!(text.contains("@tg_int_trap(i32 7)"), "{text}");
    assert!(text.contains("select i1"), "{text}");
    // One CALL site: `to_int` traps, `from_int` clamps (the declaration is
    // a second textual match and is not a site).
    assert_eq!(text.matches("call void @tg_int_trap(").count(), 1, "{text}");
}

/// The trap declaration is emitted once however many sites call it.
#[test]
fn the_trap_symbol_is_declared_once() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);
    let (a, _) = operands(&codegen);
    codegen.compile_int_bin(IntBinOp::Add, a, a).unwrap();
    codegen.compile_int_bin(IntBinOp::Mul, a, a).unwrap();
    let text = ir(&codegen);
    assert_eq!(
        text.matches("declare void @tg_int_trap").count(),
        1,
        "{text}"
    );
}
