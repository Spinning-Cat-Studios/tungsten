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

/// A nullary 2-constructor ADT must lower IDENTICALLY via its name
/// (`TyVar("Verdict")`) and via its structural `Sum` encoding — the
/// elaborator encodes n=2 ADTs as `Sum` and constructs them with inl/inr,
/// so a named-vs-structural layout split makes a call result disagree with
/// a locally-constructed merge arm (the ADR 2.7.26b T2 hard error; found
/// via the list-comparator spine, where the named route still used the
/// W4 typed payload while inl/inr used the W5 blob).
#[test]
fn nullary_two_ctor_adt_named_and_structural_lowerings_agree() {
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);

    // Verdict = Pass | Fail(String) — payload sizes differ, so a typed
    // (W4) layout would be visibly different from the [N x i8] blob.
    let mut adts = HashMap::new();
    adts.insert(
        "Verdict".to_string(),
        (
            vec![],
            vec![
                CodegenConstructor {
                    name: "Pass".to_string(),
                    fields: vec![],
                    index: 0,
                },
                CodegenConstructor {
                    name: "Fail".to_string(),
                    fields: vec![Type::String],
                    index: 1,
                },
            ],
        ),
    );
    lowering.register_adt_types(adts);

    let named = lowering.lower_type(&Type::TyVar("Verdict".to_string()));
    let structural = lowering.lower_type(&Type::sum(Type::Unit, Type::String));
    assert_eq!(
        named, structural,
        "named ADT reference and structural Sum encoding lowered differently"
    );

    // And the agreed form is the W5 blob: { i32, [16 x i8] }.
    let data_field = named.into_struct_type().get_field_type_at_index(1).unwrap();
    assert!(
        data_field.is_array_type(),
        "W5: 2-ctor ADT data field should be [N x i8], got {data_field:?}"
    );
}

/// Every spelling of a 3+-constructor ADT must lower identically too:
/// the nullary named reference (`TyVar`) and the `Type::App` route go through
/// the shared `tagged_union_blob_type` authority — `lower_app`'s n≥3 branch
/// was the last W4 (typed-payload) island after 16d2f4f1, and a typed layout
/// here would disagree with any blob-lowered spelling in a merge.
#[test]
fn three_ctor_adt_named_and_app_lowerings_agree_as_blob() {
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);

    // Shape = Dot | Seg(Nat) | Label(String) — three variants, mixed sizes.
    let shape_ctors = vec![
        CodegenConstructor {
            name: "Dot".to_string(),
            fields: vec![],
            index: 0,
        },
        CodegenConstructor {
            name: "Seg".to_string(),
            fields: vec![Type::Nat],
            index: 1,
        },
        CodegenConstructor {
            name: "Label".to_string(),
            fields: vec![Type::String],
            index: 2,
        },
    ];
    let mut adts = HashMap::new();
    adts.insert("Shape".to_string(), (vec![], shape_ctors));
    lowering.register_adt_types(adts);

    let named = lowering.lower_type(&Type::TyVar("Shape".to_string()));
    let via_app = lowering.lower_type(&Type::App("Shape".to_string(), vec![]));
    assert_eq!(
        named, via_app,
        "named reference and Type::App route lowered differently"
    );

    // The agreed form is the W5 blob { i32, [16 x i8] } (largest = String).
    let data_field = named.into_struct_type().get_field_type_at_index(1).unwrap();
    assert!(
        data_field.is_array_type(),
        "W5: 3-ctor ADT data field should be [N x i8], got {data_field:?}"
    );
    assert_eq!(data_field.into_array_type().len(), 16);
}
