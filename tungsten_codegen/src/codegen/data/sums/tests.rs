use super::CaseBranch;
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

    // Declare malloc for fold operations
    let i64_type = context.i64_type();
    let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
    let malloc_type = ptr_type.fn_type(&[i64_type.into()], false);
    codegen.module.add_function("malloc", malloc_type, None);

    codegen
}

#[test]
fn test_compile_inl_nat() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create inl[Nat + Bool](42) - inject Nat into Nat + Bool sum
    let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
    let val = Term::NatLit(42);

    let result = codegen.compile_inl(&sum_ty, &val).unwrap();

    // Result should be a struct value (sum type)
    assert!(result.is_struct_value());

    // Sum type struct should have 2 fields: i32 tag + data array
    let struct_val = result.into_struct_value();
    assert_eq!(struct_val.get_type().count_fields(), 2);
}

#[test]
fn test_compile_inr_bool() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create inr[Nat + Bool](true) - inject Bool into Nat + Bool sum
    let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
    let val = Term::True;

    let result = codegen.compile_inr(&sum_ty, &val).unwrap();

    // Result should be a struct value (sum type)
    assert!(result.is_struct_value());

    // Sum type struct should have 2 fields: i32 tag + data array
    let struct_val = result.into_struct_value();
    assert_eq!(struct_val.get_type().count_fields(), 2);
}

#[test]
fn test_compile_inl_nested_sum() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create inl[(Nat + Bool) + String](inl[Nat + Bool](1))
    // Nested sum injection
    let inner_sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
    let _outer_sum_ty = Type::Sum(Box::new(inner_sum_ty.clone()), Box::new(Type::String));

    // First create the inner inl
    let inner_val = Term::NatLit(1);
    let inner_result = codegen.compile_inl(&inner_sum_ty, &inner_val).unwrap();
    assert!(inner_result.is_struct_value());

    // The inner result is a struct, which can then be used as payload for outer inl
    // (In practice, this would be Inl(outer_sum_ty, Box::new(Inl(inner_sum_ty, ...))))
    // For this test, just verify the struct was created correctly
    assert_eq!(
        inner_result.into_struct_value().get_type().count_fields(),
        2
    );
}

#[test]
fn test_sum_type_layout_i32_tag() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create a sum type and verify its LLVM representation uses i32 tag
    let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
    let llvm_ty = codegen.types.lower_type(&sum_ty).into_struct_type();

    // Should have 2 fields
    assert_eq!(llvm_ty.count_fields(), 2);

    // First field should be i32 (tag)
    let tag_type = llvm_ty.get_field_type_at_index(0).unwrap();
    assert!(tag_type.is_int_type());
    assert_eq!(tag_type.into_int_type().get_bit_width(), 32);

    // W5: Second field should be opaque [N x i8] for ABI safety
    let data_type = llvm_ty.get_field_type_at_index(1).unwrap();
    assert!(
        data_type.is_array_type(),
        "W5: data field should be [N x i8] array, got {:?}",
        data_type
    );
    assert_eq!(
        data_type.into_array_type().len(),
        8,
        "W5: max(Nat=8, Bool=1) = [8 x i8]"
    );
}

#[test]
fn test_compile_case_simple() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create: case (inl[Nat + Bool] 42) of inl x => x | inr y => 0
    // This should evaluate to 42
    let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
    let scrut = Term::Inl(sum_ty.clone(), Box::new(Term::NatLit(42)));
    let left_branch = Term::Var("x".to_string());
    let right_branch = Term::NatLit(0);

    let left = CaseBranch {
        var: "x",
        body: &left_branch,
    };
    let right = CaseBranch {
        var: "y",
        body: &right_branch,
    };
    let result = codegen.compile_case(&scrut, &left, &right, false);

    // Should compile successfully
    assert!(result.is_ok());
}

#[test]
fn test_extract_sum_variant_types_valid() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
    let result = codegen.extract_sum_variant_types(&sum_ty);

    assert!(result.is_ok());
    let (left, right) = result.unwrap();
    assert_eq!(left, Type::Nat);
    assert_eq!(right, Type::Bool);
}

#[test]
fn test_extract_sum_variant_types_nested() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    // (Nat + Bool) + String
    let inner = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
    let outer = Type::Sum(Box::new(inner.clone()), Box::new(Type::String));
    let result = codegen.extract_sum_variant_types(&outer);

    assert!(result.is_ok());
    let (left, right) = result.unwrap();
    assert_eq!(left, inner);
    assert_eq!(right, Type::String);
}

#[test]
fn test_extract_sum_variant_types_non_sum_error() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    let nat_ty = Type::Nat;
    let result = codegen.extract_sum_variant_types(&nat_ty);

    assert!(result.is_err());
}

#[test]
fn test_compile_case_right_branch() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create: case (inr[Nat + Bool] true) of inl x => 0 | inr y => 1
    // Tests right branch compilation
    let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
    let scrut = Term::Inr(sum_ty.clone(), Box::new(Term::True));
    let left_branch = Term::NatLit(0);
    let right_branch = Term::NatLit(1);

    let left = CaseBranch {
        var: "x",
        body: &left_branch,
    };
    let right = CaseBranch {
        var: "y",
        body: &right_branch,
    };
    let result = codegen.compile_case(&scrut, &left, &right, false);
    assert!(result.is_ok());
}

#[test]
fn test_compile_case_nested_sum() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Test case on nested sum: (Unit + Nat) + Bool
    let inner_sum = Type::Sum(Box::new(Type::Unit), Box::new(Type::Nat));
    let outer_sum = Type::Sum(Box::new(inner_sum.clone()), Box::new(Type::Bool));

    // inl[inner_sum + Bool](inl[Unit + Nat](()))
    let inner_scrut = Term::Inl(inner_sum.clone(), Box::new(Term::Unit));
    let scrut = Term::Inl(outer_sum.clone(), Box::new(inner_scrut));

    // Both branches return Nat
    let left_branch = Term::NatLit(42);
    let right_branch = Term::NatLit(0);

    let left = CaseBranch {
        var: "x",
        body: &left_branch,
    };
    let right = CaseBranch {
        var: "y",
        body: &right_branch,
    };
    let result = codegen.compile_case(&scrut, &left, &right, false);
    assert!(result.is_ok());
}

#[test]
fn test_compile_case_with_variable_binding() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Test that variable binding works: inl x => x uses the bound value
    let sum_ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::Nat));
    let scrut = Term::Inl(sum_ty.clone(), Box::new(Term::NatLit(99)));

    // Both branches use their bound variable
    let left_branch = Term::Var("x".to_string());
    let right_branch = Term::Var("y".to_string());

    let left = CaseBranch {
        var: "x",
        body: &left_branch,
    };
    let right = CaseBranch {
        var: "y",
        body: &right_branch,
    };
    let result = codegen.compile_case(&scrut, &left, &right, false);
    assert!(result.is_ok());
}

// ── G2 Test 4: Phi merge branch type equality (ADR 11.4.26b) ──

#[test]
fn test_compile_case_branch_types_match() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Case on Sum(Unit, String) where branches return Nat.
    // Both branches must produce values with identical LLVM types at the phi merge.
    let sum_ty = Type::Sum(Box::new(Type::Unit), Box::new(Type::String));
    let scrut = Term::Inl(sum_ty.clone(), Box::new(Term::Unit));

    // Both branches return Nat literals — types should match at phi
    let left_branch = Term::NatLit(1);
    let right_branch = Term::NatLit(2);

    let left = CaseBranch {
        var: "x",
        body: &left_branch,
    };
    let right = CaseBranch {
        var: "y",
        body: &right_branch,
    };
    let result = codegen.compile_case(&scrut, &left, &right, false);
    assert!(result.is_ok());

    let result_val = result.unwrap();
    // The result type is the phi merge type — it should be consistent
    // (both branches produce i64, so the merge should too)
    assert!(
        result_val.is_int_value(),
        "Phi merge of two Nat branches should produce an int, got {:?}",
        result_val.get_type()
    );
}

#[test]
fn test_compile_case_asymmetric_sum_branch_types_match() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Case on Sum(Bool, String) — highly asymmetric payload sizes (1 vs 16 bytes).
    // Both branches return Nat, so phi merge types must still be identical.
    let sum_ty = Type::Sum(Box::new(Type::Bool), Box::new(Type::String));
    let scrut = Term::Inr(sum_ty.clone(), Box::new(Term::string_lit("hello")));

    let left_branch = Term::NatLit(0);
    let right_branch = Term::NatLit(1);

    let left = CaseBranch {
        var: "x",
        body: &left_branch,
    };
    let right = CaseBranch {
        var: "y",
        body: &right_branch,
    };
    let result = codegen.compile_case(&scrut, &left, &right, false);
    assert!(result.is_ok());

    let result_val = result.unwrap();
    assert!(
        result_val.is_int_value(),
        "Phi merge of two Nat branches should produce an int, got {:?}",
        result_val.get_type()
    );
}
