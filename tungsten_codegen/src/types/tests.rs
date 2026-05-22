use super::*;
use inkwell::context::Context;
use inkwell::types::BasicTypeEnum;
use std::collections::HashMap;

#[test]
fn test_lower_bool() {
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);
    let llvm_ty = lowering.lower_type(&Type::Bool);
    assert!(llvm_ty.is_int_type());
}

#[test]
fn test_lower_nat() {
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);
    let llvm_ty = lowering.lower_type(&Type::Nat);
    assert!(llvm_ty.is_int_type());
    if let BasicTypeEnum::IntType(int_ty) = llvm_ty {
        assert_eq!(int_ty.get_bit_width(), 64);
    }
}

#[test]
fn test_lower_product() {
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);
    let ty = Type::product(Type::Bool, Type::Nat);
    let llvm_ty = lowering.lower_type(&ty);
    assert!(llvm_ty.is_struct_type());
}

#[test]
fn test_lower_arrow() {
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);
    let ty = Type::arrow(Type::Nat, Type::Bool);
    let llvm_ty = lowering.lower_type(&ty);
    // Arrow types are closures (structs with fn_ptr and env_ptr)
    assert!(llvm_ty.is_struct_type());
}

// ── G2: Type-size consistency tests (ADR 11.4.26b) ──

#[test]
fn test_lower_sum_payload_size_uses_max() {
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);

    // Sum(Unit, String) — Unit=0 bytes, String=16 bytes
    // W5: Payload field should be opaque [N x i8] for ABI safety
    let sum_ty = Type::Sum(Box::new(Type::Unit), Box::new(Type::String));
    let llvm_ty = lowering.lower_type(&sum_ty).into_struct_type();

    // Second field is [16 x i8] (max of Unit=0, String=16)
    let data_field = llvm_ty.get_field_type_at_index(1).unwrap();
    assert!(
        data_field.is_array_type(),
        "W5: Sum(Unit, String) data field should be [N x i8] array, got {:?}",
        data_field
    );

    let data_array = data_field.into_array_type();
    assert_eq!(
        data_array.len(),
        16,
        "W5: Sum(Unit, String) data field should be [16 x i8]"
    );
}

#[test]
fn test_lower_sum_asymmetric_sizes() {
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);

    // Sum(Bool, String) — Bool=1 byte, String=16 bytes
    // W5: Payload field should be opaque [N x i8] for ABI safety
    let sum_ty = Type::Sum(Box::new(Type::Bool), Box::new(Type::String));
    let llvm_ty = lowering.lower_type(&sum_ty).into_struct_type();

    let data_field = llvm_ty.get_field_type_at_index(1).unwrap();
    assert!(
        data_field.is_array_type(),
        "W5: Sum(Bool, String) data field should be [N x i8] array, got {:?}",
        data_field
    );
}

#[test]
fn test_lower_adt_option_string_size() {
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);

    // Register Option ADT: None() | Some(T)
    let mut adts = HashMap::new();
    adts.insert(
        "Option".to_string(),
        (
            vec!["T".to_string()],
            vec![
                CodegenConstructor {
                    name: "None".to_string(),
                    fields: vec![],
                    index: 0,
                },
                CodegenConstructor {
                    name: "Some".to_string(),
                    fields: vec![Type::TyVar("T".to_string())],
                    index: 1,
                },
            ],
        ),
    );
    lowering.register_adt_types(adts);

    // Lower Option<String>
    let option_string = Type::App("Option".to_string(), vec![Type::String]);
    let llvm_ty = lowering.lower_type(&option_string).into_struct_type();

    // Should have 2 fields: i32 tag + data
    assert_eq!(llvm_ty.count_fields(), 2);

    // W5: Data field should be opaque [N x i8] for ABI safety.
    // For Option<String>, the max payload is String = 16 bytes.
    let data_field = llvm_ty.get_field_type_at_index(1).unwrap();
    assert!(
        data_field.is_array_type(),
        "W5: Option<String> data field should be [N x i8] array, got {:?}",
        data_field
    );

    let data_array = data_field.into_array_type();
    assert_eq!(
        data_array.len(),
        16,
        "W5: Option<String> data field should be [16 x i8]"
    );
}
