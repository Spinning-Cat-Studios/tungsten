//! ADR 1.7.26f — `declare_referenced_externs` unit tests (R1/R5).
//!
//! Exercises the shared cross-unit declaration helper directly against a
//! minimal `UnitCompileCtx`: idempotency (double-call declares once, a type
//! mismatch is a hard error), Forall registration without a prototype,
//! own-unit skipping, and extern-name-map population for scoped names.

use super::support::{count_declares_of, def_info, TestCtxParts};
use crate::compile::per_module::compilation::declare_referenced_externs;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use tungsten_codegen::inkwell::context::Context;
use tungsten_codegen::CodeGen;
use tungsten_core::types::Type;

#[test]
fn double_call_declares_prototype_once() {
    let mut defs = BTreeMap::new();
    defs.insert(
        "helpers__assert::assert".to_string(),
        def_info(
            "assert",
            Type::arrow(Type::Nat, Type::Nat),
            "helpers__assert",
        ),
    );
    let parts = TestCtxParts::with_defs(defs);
    let referenced: BTreeSet<String> = ["assert".to_string()].into();

    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "__mono");
    let mut extern_map = HashMap::new();

    for _ in 0..2 {
        declare_referenced_externs(
            &mut codegen,
            &referenced,
            &parts.ctx(),
            "__mono",
            &mut extern_map,
        )
        .expect("declare_referenced_externs failed");
    }

    let ir = codegen.get_ir_string();
    assert_eq!(
        count_declares_of(&ir, "assert"),
        1,
        "expected exactly one prototype for 'assert', IR:\n{ir}"
    );
    assert!(
        !ir.contains("@\"assert.1\"") && !ir.contains("@assert.1"),
        "duplicate renamed prototype emitted:\n{ir}"
    );
}

#[test]
fn mismatched_existing_type_is_hard_error() {
    let mut first_defs = BTreeMap::new();
    first_defs.insert(
        "helpers__assert::assert".to_string(),
        def_info(
            "assert",
            Type::arrow(Type::Nat, Type::Nat),
            "helpers__assert",
        ),
    );
    let mut second_defs = BTreeMap::new();
    second_defs.insert(
        "helpers__assert::assert".to_string(),
        def_info(
            "assert",
            Type::arrow(Type::Nat, Type::arrow(Type::Nat, Type::Nat)),
            "helpers__assert",
        ),
    );
    let first = TestCtxParts::with_defs(first_defs);
    let second = TestCtxParts::with_defs(second_defs);
    let referenced: BTreeSet<String> = ["assert".to_string()].into();

    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "__mono");
    let mut extern_map = HashMap::new();

    declare_referenced_externs(
        &mut codegen,
        &referenced,
        &first.ctx(),
        "__mono",
        &mut extern_map,
    )
    .expect("first declaration failed");
    let err = declare_referenced_externs(
        &mut codegen,
        &referenced,
        &second.ctx(),
        "__mono",
        &mut extern_map,
    )
    .expect_err("mismatched re-declaration must be a hard error");
    assert!(
        err.contains("conflicting declarations"),
        "unexpected error text: {err}"
    );
}

#[test]
fn forall_def_registers_type_without_prototype() {
    let generic_ty = Type::Forall(
        "T".to_string(),
        Box::new(Type::arrow(
            Type::TyVar("T".to_string()),
            Type::TyVar("T".to_string()),
        )),
    );
    let mut defs = BTreeMap::new();
    defs.insert(
        "util__id::id".to_string(),
        def_info("id", generic_ty.clone(), "util__id"),
    );
    let parts = TestCtxParts::with_defs(defs);
    let referenced: BTreeSet<String> = ["id".to_string()].into();

    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "__mono");
    let mut extern_map = HashMap::new();

    declare_referenced_externs(
        &mut codegen,
        &referenced,
        &parts.ctx(),
        "__mono",
        &mut extern_map,
    )
    .expect("declare_referenced_externs failed");

    assert_eq!(codegen.get_def_type("id"), Some(generic_ty));
    let ir = codegen.get_ir_string();
    assert_eq!(
        count_declares_of(&ir, "id"),
        0,
        "generic def must not get a prototype (it monomorphizes on demand):\n{ir}"
    );
}

#[test]
fn own_unit_defs_and_unreferenced_defs_are_skipped() {
    let mut defs = BTreeMap::new();
    defs.insert(
        "__mono::own_def".to_string(),
        def_info("own_def", Type::arrow(Type::Nat, Type::Nat), "__mono"),
    );
    defs.insert(
        "util__unused::unused".to_string(),
        def_info("unused", Type::arrow(Type::Nat, Type::Nat), "util__unused"),
    );
    let parts = TestCtxParts::with_defs(defs);
    let referenced: BTreeSet<String> = ["own_def".to_string()].into();

    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "__mono");
    let mut extern_map = HashMap::new();

    declare_referenced_externs(
        &mut codegen,
        &referenced,
        &parts.ctx(),
        "__mono",
        &mut extern_map,
    )
    .expect("declare_referenced_externs failed");

    let ir = codegen.get_ir_string();
    assert_eq!(
        count_declares_of(&ir, "own_def"),
        0,
        "own-unit def declared"
    );
    assert_eq!(
        count_declares_of(&ir, "unused"),
        0,
        "unreferenced def declared"
    );
    assert!(extern_map.is_empty());
}

#[test]
fn scoped_llvm_name_lands_in_extern_map() {
    let mut defs = BTreeMap::new();
    defs.insert(
        "a__describe::describe".to_string(),
        def_info(
            "a__describe__describe",
            Type::arrow(Type::Nat, Type::Nat),
            "a__describe",
        ),
    );
    let parts = TestCtxParts::with_defs(defs);
    let referenced: BTreeSet<String> = ["describe".to_string()].into();

    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "__mono");
    let mut extern_map = HashMap::new();

    declare_referenced_externs(
        &mut codegen,
        &referenced,
        &parts.ctx(),
        "__mono",
        &mut extern_map,
    )
    .expect("declare_referenced_externs failed");

    assert_eq!(
        extern_map.get("describe").map(String::as_str),
        Some("a__describe__describe")
    );
    let ir = codegen.get_ir_string();
    assert_eq!(count_declares_of(&ir, "a__describe__describe"), 1);
}
