//! ABI passing mode tests: direct vs indirect classification.

use std::collections::HashMap;

use crate::dump_abi::analyze::analyze_function;
use crate::dump_abi::PassingMode;

#[test]
fn abi_small_struct_is_direct() {
    let mut types = HashMap::new();
    types.insert("Ordering".to_string(), "{ i32, [0 x i8] }".to_string());

    let sig = "define %Ordering @compare(%Ordering %a, %Ordering %b)";
    let func = analyze_function(
        "compare",
        sig,
        &crate::diff_ir::parser::IrDefs {
            types,
            functions: HashMap::new(),
        },
    )
    .unwrap();

    assert_eq!(func.params.len(), 2);
    assert_eq!(func.params[0].passing, PassingMode::Direct);
    assert_eq!(func.params[1].passing, PassingMode::Direct);
    assert_eq!(func.ret.passing, PassingMode::Direct);
}

#[test]
fn abi_large_struct_is_indirect() {
    let mut types = HashMap::new();
    types.insert("Item".to_string(), "{ i32, [88 x i8] }".to_string());

    let sig = "define i64 @register_type_name(%Item %item)";
    let func = analyze_function(
        "register_type_name",
        sig,
        &crate::diff_ir::parser::IrDefs {
            types,
            functions: HashMap::new(),
        },
    )
    .unwrap();

    assert_eq!(func.params.len(), 1);
    assert_eq!(func.params[0].passing, PassingMode::Indirect);
    assert_eq!(func.ret.passing, PassingMode::Direct);
}

#[test]
fn abi_primitive_always_direct() {
    let types = HashMap::new();
    let sig = "define i64 @id(i64 %x)";
    let func = analyze_function(
        "id",
        sig,
        &crate::diff_ir::parser::IrDefs {
            types,
            functions: HashMap::new(),
        },
    )
    .unwrap();

    assert_eq!(func.params[0].passing, PassingMode::Direct);
    assert_eq!(func.ret.passing, PassingMode::Direct);
}

#[test]
fn abi_16_byte_struct_is_direct() {
    let types = HashMap::new();
    // { i64, i64 } = exactly 16 bytes → DIRECT
    let sig = "define void @f({ i64, i64 } %s)";
    let func = analyze_function(
        "f",
        sig,
        &crate::diff_ir::parser::IrDefs {
            types,
            functions: HashMap::new(),
        },
    )
    .unwrap();

    assert_eq!(func.params[0].passing, PassingMode::Direct);
}

#[test]
fn abi_17_byte_struct_is_indirect() {
    let types = HashMap::new();
    // { i64, i64, i8 } = 17+ bytes → INDIRECT
    let sig = "define void @f({ i64, i64, i8 } %s)";
    let func = analyze_function(
        "f",
        sig,
        &crate::diff_ir::parser::IrDefs {
            types,
            functions: HashMap::new(),
        },
    )
    .unwrap();

    // { i64, i64, i8 } → size = 24 (8+8+1, rounded to 8) → INDIRECT
    assert_eq!(func.params[0].passing, PassingMode::Indirect);
}
