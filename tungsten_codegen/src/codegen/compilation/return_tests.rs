//! Tests for `compile_return`'s sret out-pointer path (ADR 3.7.26a).
//!
//! In a void-returning `$direct_mt` body the return value travels through
//! the sret out-pointer (param 0, ADR 1.7.26a). Before the fix,
//! `compile_return` emitted a bare `ret void` there, silently discarding the
//! value and leaving the caller's sret buffer uninitialized.

use crate::codegen::CodeGen;
use inkwell::context::Context;
use tungsten_core::terms::Term;

#[test]
fn early_return_in_sret_function_stores_through_out_pointer() {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "sret_return_test");

    // void fn(ptr) — an sret-style $direct_mt entry with the out-pointer at
    // param 0 and an active current_sret_type (as set by
    // compile_decomposed_entry).
    let ptr_ty = context.ptr_type(inkwell::AddressSpace::default());
    let fn_type = context.void_type().fn_type(&[ptr_ty.into()], false);
    let function = codegen.module.add_function("mt_fn", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);

    let ret_struct = context.struct_type(&[context.i64_type().into()], false);
    codegen.compilation.current_sret_type = Some(ret_struct.into());

    codegen
        .compile_term(&Term::Return(Box::new(Term::NatLit(7))))
        .unwrap();

    let ir = codegen.module.print_to_string().to_string();
    assert!(
        ir.contains("store"),
        "sret early return must store through the out-pointer, got:\n{ir}"
    );
    assert!(
        ir.contains("ret void"),
        "sret function must still return void, got:\n{ir}"
    );
}

#[test]
fn early_return_in_plain_void_function_stays_bare_ret_void() {
    // Without an active sret out-pointer, a void function's return keeps the
    // bare `ret void` (no store emitted).
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "void_return_test");

    let fn_type = context.void_type().fn_type(&[], false);
    let function = codegen.module.add_function("void_fn", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);
    codegen.compilation.current_sret_type = None;

    codegen
        .compile_term(&Term::Return(Box::new(Term::Unit)))
        .unwrap();

    let ir = codegen.module.print_to_string().to_string();
    assert!(
        !ir.contains("store "),
        "plain void return must not store, got:\n{ir}"
    );
    assert!(ir.contains("ret void"), "expected ret void, got:\n{ir}");
}
