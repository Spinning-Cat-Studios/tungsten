use super::*;
use inkwell::context::Context;
use tungsten_core::terms::Term;

fn setup_codegen_with_function(context: &Context) -> CodeGen<'_> {
    let mut codegen = CodeGen::new(context, "test");

    // Create a simple function to provide a basic block context
    let void_type = context.void_type();
    let fn_type = void_type.fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);

    // Declare malloc for fold operations (recursive ADTs)
    let i64_type = context.i64_type();
    let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
    let malloc_type = ptr_type.fn_type(&[i64_type.into()], false);
    codegen.module.add_function("malloc", malloc_type, None);

    codegen
}

/// Create a simple non-recursive ADT type for testing:
/// type Color = | Red(Unit) | Green(Unit) | Blue(Unit)
fn make_color_adt() -> Type {
    Type::Adt(
        "Color".to_string(),
        vec![], // no type args
        vec![
            ("Red".to_string(), Type::Unit),
            ("Green".to_string(), Type::Unit),
            ("Blue".to_string(), Type::Unit),
        ],
    )
}

/// Create a simple ADT with different payload types:
/// type Shape = | Circle(Nat) | Rectangle(Nat, Nat) | Triangle(Bool)
fn make_shape_adt() -> Type {
    Type::Adt(
        "Shape".to_string(),
        vec![],
        vec![
            ("Circle".to_string(), Type::Nat), // radius
            (
                "Rectangle".to_string(),
                Type::Product(Box::new(Type::Nat), Box::new(Type::Nat)),
            ), // width, height
            ("Triangle".to_string(), Type::Bool), // is_equilateral
        ],
    )
}

#[test]
fn test_adt_type_layout_i32_tag() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // ADT type with inline variant info - no registration needed
    let adt_ty = make_color_adt();
    let llvm_ty = codegen.types.lower_type(&adt_ty).into_struct_type();

    // Should have 2 fields: i32 tag + data
    assert_eq!(llvm_ty.count_fields(), 2);

    // First field should be i32 (tag)
    let tag_type = llvm_ty.get_field_type_at_index(0).unwrap();
    assert!(tag_type.is_int_type());
    assert_eq!(tag_type.into_int_type().get_bit_width(), 32);

    // W5: Second field should be [N x i8] byte array for ABI-safe data passing.
    // Color has 3 Unit variants, so the largest payload is 0 bytes → [0 x i8].
    let data_type = llvm_ty.get_field_type_at_index(1).unwrap();
    assert!(
        data_type.is_array_type(),
        "W5: Color ADT data field should be [N x i8] array, got {:?}",
        data_type
    );
}

#[test]
fn test_compile_adt_construct_simple() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create the Color ADT - variants are inline in the type
    let adt_ty = make_color_adt();

    // Construct Color::Green(())
    let payload = Term::Unit;
    let result = codegen.compile_adt_construct(&adt_ty, 1, &payload).unwrap();

    // Result should be a struct value
    assert!(result.is_struct_value());

    // Struct should have 2 fields
    let struct_val = result.into_struct_value();
    assert_eq!(struct_val.get_type().count_fields(), 2);
}

#[test]
fn test_compile_adt_construct_with_nat_payload() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Shape ADT - variants are inline in the type
    let adt_ty = make_shape_adt();

    // Construct Shape::Circle(42)
    let payload = Term::NatLit(42);
    let result = codegen.compile_adt_construct(&adt_ty, 0, &payload).unwrap();

    // Result should be a struct value
    assert!(result.is_struct_value());
}

#[test]
fn test_get_adt_variants() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    let adt_ty = make_color_adt();
    let variants = codegen.get_adt_variants(&adt_ty).unwrap();

    // Should have 3 variants
    assert_eq!(variants.len(), 3);

    // Check variant names and payload types
    assert_eq!(variants[0].0, "Red");
    assert_eq!(variants[0].1, Type::Unit);

    assert_eq!(variants[1].0, "Green");
    assert_eq!(variants[1].1, Type::Unit);

    assert_eq!(variants[2].0, "Blue");
    assert_eq!(variants[2].1, Type::Unit);
}

#[test]
fn test_get_adt_variants_non_adt_error() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    // Try to get variants from a non-ADT type
    let nat_ty = Type::Nat;
    let result = codegen.get_adt_variants(&nat_ty);

    // Should return an error
    assert!(result.is_err());
}

#[test]
fn test_adt_match_exhaustive() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Color ADT - variants are inline in the type
    let adt_ty = make_color_adt();

    // Create scrutinee: Color::Red(())
    let scrut = Term::AdtConstruct(
        adt_ty.clone(),
        0, // Red
        Box::new(Term::Unit),
    );

    // Create exhaustive match arms that return Nat
    // match color { Red(_) => 0, Green(_) => 1, Blue(_) => 2 }
    let arms = vec![
        (0, "r".to_string(), Box::new(Term::NatLit(0))),
        (1, "g".to_string(), Box::new(Term::NatLit(1))),
        (2, "b".to_string(), Box::new(Term::NatLit(2))),
    ];

    let result = codegen.compile_adt_match(&scrut, &arms, false);

    // Should compile successfully
    assert!(result.is_ok());
}

#[test]
fn test_adt_match_with_payload_binding() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Shape ADT with different payload types
    let adt_ty = make_shape_adt();

    // Create scrutinee: Shape::Circle(42)
    let scrut = Term::AdtConstruct(
        adt_ty.clone(),
        0, // Circle
        Box::new(Term::NatLit(42)),
    );

    // Match arms that use the bound payload
    // match shape { Circle(r) => r, Rectangle(dims) => 0, Triangle(b) => 1 }
    let arms = vec![
        (0, "r".to_string(), Box::new(Term::Var("r".to_string()))), // uses payload
        (1, "dims".to_string(), Box::new(Term::NatLit(0))),
        (2, "b".to_string(), Box::new(Term::NatLit(1))),
    ];

    let result = codegen.compile_adt_match(&scrut, &arms, false);
    assert!(result.is_ok());
}

#[test]
fn test_get_adt_variants_shape() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    let adt_ty = make_shape_adt();
    let variants = codegen.get_adt_variants(&adt_ty).unwrap();

    assert_eq!(variants.len(), 3);

    // Verify variant names and payload types
    assert_eq!(variants[0].0, "Circle");
    assert_eq!(variants[0].1, Type::Nat);

    assert_eq!(variants[1].0, "Rectangle");
    assert!(matches!(variants[1].1, Type::Product(_, _)));

    assert_eq!(variants[2].0, "Triangle");
    assert_eq!(variants[2].1, Type::Bool);
}

#[test]
fn test_compile_adt_construct_all_variants() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let adt_ty = make_color_adt();

    // Test constructing all three variants
    for idx in [0, 1, 2] {
        let result = codegen.compile_adt_construct(&adt_ty, idx, &Term::Unit);
        assert!(
            result.is_ok(),
            "Failed to construct variant at index {}",
            idx
        );
        assert!(result.unwrap().is_struct_value());
    }
}

#[test]
fn test_compile_adt_construct_with_product_payload() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let adt_ty = make_shape_adt();

    // Construct Shape::Rectangle((10, 20))
    let payload = Term::Pair(Box::new(Term::NatLit(10)), Box::new(Term::NatLit(20)));
    let result = codegen.compile_adt_construct(&adt_ty, 1, &payload);

    assert!(result.is_ok());
    assert!(result.unwrap().is_struct_value());
}

#[test]
fn test_type_mentions_named_var_simple() {
    // Test direct TyVar
    assert!(CodeGen::type_mentions_named_var(
        &Type::TyVar("X".to_string()),
        "X"
    ));
    assert!(!CodeGen::type_mentions_named_var(
        &Type::TyVar("Y".to_string()),
        "X"
    ));
}

#[test]
fn test_type_mentions_named_var_nested() {
    // Test nested in Arrow
    let arrow_ty = Type::Arrow(Box::new(Type::TyVar("X".to_string())), Box::new(Type::Nat));
    assert!(CodeGen::type_mentions_named_var(&arrow_ty, "X"));
    assert!(!CodeGen::type_mentions_named_var(&arrow_ty, "Y"));

    // Test nested in Product
    let prod_ty = Type::Product(Box::new(Type::Nat), Box::new(Type::TyVar("X".to_string())));
    assert!(CodeGen::type_mentions_named_var(&prod_ty, "X"));
}

#[test]
fn test_type_mentions_named_var_terminal_types() {
    // Terminal types should not mention any type variable
    assert!(!CodeGen::type_mentions_named_var(&Type::Nat, "X"));
    assert!(!CodeGen::type_mentions_named_var(&Type::Bool, "X"));
    assert!(!CodeGen::type_mentions_named_var(&Type::String, "X"));
    assert!(!CodeGen::type_mentions_named_var(&Type::Unit, "X"));
}

#[test]
fn test_adt_match_partial_arms() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let adt_ty = make_color_adt();

    // Create scrutinee: Color::Green(())
    let scrut = Term::AdtConstruct(
        adt_ty.clone(),
        1, // Green
        Box::new(Term::Unit),
    );

    // Only match on two variants (partial match - Red and Green only)
    // In practice, this should ideally be caught at type-check time,
    // but the codegen should still work
    let arms = vec![
        (0, "r".to_string(), Box::new(Term::NatLit(0))),
        (1, "g".to_string(), Box::new(Term::NatLit(1))),
    ];

    let result = codegen.compile_adt_match(&scrut, &arms, false);
    // Should compile (default case goes to unreachable)
    assert!(result.is_ok());
}

/// Create a 5-variant ADT to test larger switches:
/// type Weekday = | Mon(Unit) | Tue(Unit) | Wed(Unit) | Thu(Unit) | Fri(Unit)
fn make_weekday_adt() -> Type {
    Type::Adt(
        "Weekday".to_string(),
        vec![],
        vec![
            ("Mon".to_string(), Type::Unit),
            ("Tue".to_string(), Type::Unit),
            ("Wed".to_string(), Type::Unit),
            ("Thu".to_string(), Type::Unit),
            ("Fri".to_string(), Type::Unit),
        ],
    )
}

#[test]
fn test_adt_match_five_variants() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    let adt_ty = make_weekday_adt();

    // Create scrutinee: Weekday::Wed(())
    let scrut = Term::AdtConstruct(
        adt_ty.clone(),
        2, // Wed
        Box::new(Term::Unit),
    );

    // Match all five variants
    let arms = vec![
        (0, "m".to_string(), Box::new(Term::NatLit(1))),
        (1, "t".to_string(), Box::new(Term::NatLit(2))),
        (2, "w".to_string(), Box::new(Term::NatLit(3))),
        (3, "th".to_string(), Box::new(Term::NatLit(4))),
        (4, "f".to_string(), Box::new(Term::NatLit(5))),
    ];

    let result = codegen.compile_adt_match(&scrut, &arms, false);
    assert!(result.is_ok());
}
