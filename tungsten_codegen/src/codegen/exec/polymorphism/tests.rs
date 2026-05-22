use super::*;
use inkwell::context::Context;

fn setup_codegen_with_function(context: &Context) -> CodeGen {
    let mut codegen = CodeGen::new(context, "test");

    // Create a function context for builder operations
    let void_type = context.void_type();
    let fn_type = void_type.fn_type(&[], false);
    let function = codegen.module.add_function("test_fn", fn_type, None);
    let entry = context.append_basic_block(function, "entry");
    codegen.builder.position_at_end(entry);
    codegen.compilation.current_fn = Some(function);

    codegen
}

#[test]
fn test_compile_ty_app_tyabs() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Create TyAbs("T", NatLit(42))
    let term = Term::TyAbs("T".to_string(), Box::new(Term::NatLit(42)));

    // Compile TyApp(TyAbs, Nat)
    let result = codegen.compile_ty_app(&term, &Type::Nat).unwrap();
    assert!(result.is_int_value());
}

#[test]
fn test_compile_ty_app_fallback() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // For non-TyAbs/non-Global terms, should fall back to compiling the term
    let term = Term::NatLit(42);
    let result = codegen.compile_ty_app(&term, &Type::Nat).unwrap();
    assert!(result.is_int_value());
}

#[test]
fn test_wrap_function_as_closure() {
    let context = Context::create();
    let codegen = setup_codegen_with_function(&context);

    // Create a dummy function
    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_type = context.i64_type().fn_type(&[ptr_type.into()], false);
    let func = codegen.module.add_function("dummy_fn", fn_type, None);

    let closure = codegen.wrap_function_as_closure(func).unwrap();
    assert!(closure.is_struct_value());
    // Closure is { ptr, ptr }
    assert_eq!(closure.into_struct_value().get_type().count_fields(), 2);
}

#[test]
fn test_monomorphization_not_polymorphic() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Register a non-polymorphic definition
    codegen
        .defs
        .def_types
        .insert("not_poly".to_string(), Type::Nat);
    codegen
        .defs
        .term_defs
        .insert("not_poly".to_string(), Term::NatLit(42));

    // Should return None (not polymorphic)
    let result = codegen
        .compile_monomorphized("not_poly", &Type::String)
        .unwrap();
    assert!(result.is_none());
}

#[test]
fn test_monomorphization_no_term() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Register type but no term
    codegen.defs.def_types.insert(
        "no_term".to_string(),
        Type::Forall("T".to_string(), Box::new(Type::TyVar("T".to_string()))),
    );

    // Should return None (no term available)
    let result = codegen
        .compile_monomorphized("no_term", &Type::Nat)
        .unwrap();
    assert!(result.is_none());
}

#[test]
fn test_register_mono_instance_preseeds_lookup() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Pre-seed a mono instance (ADR 8.5.26g §2.4)
    codegen.register_mono_instance("map", &[Type::Nat], "_tg_list_map_I_Nat");

    // Verify it appears in MonomorphState
    let ty_key = format!("{:?}", Type::Nat);
    let mono_key = ("map".to_string(), ty_key);
    assert_eq!(
        codegen.monomorph.instances.get(&mono_key),
        Some(&"_tg_list_map_I_Nat".to_string()),
    );
}

#[test]
fn test_register_mono_instance_used_by_compile_monomorphized() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Declare a function with the mono symbol name
    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_type = context.i64_type().fn_type(&[ptr_type.into()], false);
    codegen.module.add_function("_tg_m_f_I_Nat", fn_type, None);

    // Pre-seed the mono instance
    codegen.register_mono_instance("f", &[Type::Nat], "_tg_m_f_I_Nat");

    // Register f as polymorphic so compile_monomorphized would try it
    codegen.defs.def_types.insert(
        "f".to_string(),
        Type::Forall("T".to_string(), Box::new(Type::TyVar("T".to_string()))),
    );

    // compile_monomorphized should find the pre-seeded instance
    // and return a closure wrapping the existing function
    let result = codegen.compile_monomorphized("f", &Type::Nat).unwrap();
    assert!(result.is_some(), "should find pre-seeded mono instance");
    assert!(
        result.unwrap().is_struct_value(),
        "should return closure struct"
    );
}

#[test]
fn test_mono_map_active_guard_rejects_unknown_instance() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);
    codegen.set_module_prefix("test_unit".to_string());

    // Register a polymorphic definition
    codegen.defs.def_types.insert(
        "f".to_string(),
        Type::Forall("T".to_string(), Box::new(Type::TyVar("T".to_string()))),
    );
    codegen.defs.term_defs.insert(
        "f".to_string(),
        Term::TyAbs("T".to_string(), Box::new(Term::NatLit(0))),
    );

    // Activate the mono map guard
    codegen.activate_mono_map();

    // Attempt ad-hoc mono — should be rejected with ICE
    let result = codegen.compile_monomorphized("f", &Type::Nat);
    assert!(
        result.is_err(),
        "should reject ad-hoc mono when guard is active"
    );
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("ICE"), "error should be ICE: {}", err_msg);
    assert!(
        err_msg.contains("f"),
        "error should name the function: {}",
        err_msg
    );
    assert!(
        err_msg.contains("test_unit"),
        "error should include unit name: {}",
        err_msg
    );
    assert!(
        err_msg.contains("--trace-mono"),
        "error should suggest --trace-mono: {}",
        err_msg
    );
}

#[test]
fn test_mono_map_active_allows_preseeded_instance() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Pre-seed, then activate guard
    let ptr_type = context.ptr_type(AddressSpace::default());
    let fn_type = context.i64_type().fn_type(&[ptr_type.into()], false);
    codegen.module.add_function("_tg_f_I_Nat", fn_type, None);
    codegen.register_mono_instance("f", &[Type::Nat], "_tg_f_I_Nat");
    codegen.activate_mono_map();

    // Register f as polymorphic
    codegen.defs.def_types.insert(
        "f".to_string(),
        Type::Forall("T".to_string(), Box::new(Type::TyVar("T".to_string()))),
    );

    // Should succeed — instance was pre-seeded
    let result = codegen.compile_monomorphized("f", &Type::Nat).unwrap();
    assert!(result.is_some(), "pre-seeded instance should resolve");
}

#[test]
fn test_extract_poly_body_returns_none_for_non_poly() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    codegen.defs.def_types.insert("g".to_string(), Type::Nat);
    codegen
        .defs
        .term_defs
        .insert("g".to_string(), Term::NatLit(42));

    assert!(codegen.extract_poly_body("g").is_none());
    assert!(codegen.extract_poly_body("nonexistent").is_none());
}

#[test]
fn test_extract_poly_body_returns_parts_for_poly() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    codegen.defs.def_types.insert(
        "f".to_string(),
        Type::Forall("T".to_string(), Box::new(Type::Nat)),
    );
    codegen.defs.term_defs.insert(
        "f".to_string(),
        Term::TyAbs("T".to_string(), Box::new(Term::NatLit(0))),
    );

    let parts = codegen.extract_poly_body("f");
    assert!(parts.is_some());
    let (var, _body, inner_ty) = parts.unwrap();
    assert_eq!(var, "T");
    assert_eq!(inner_ty, Type::Nat);
}

/// Compound type containing @-prefixed TyVar should NOT be considered
/// mono-blocking — @Token is a Phase 1c reference to a concrete record type.
#[test]
fn test_has_mono_blocking_tyvar_compound_at_prefix() {
    let context = Context::create();
    let mut codegen = setup_codegen_with_function(&context);

    // Register "Token" as a known record type so is_concrete_named_type returns true.
    codegen
        .types
        .register_record_types([("Token".to_string(), vec![])].into_iter().collect());

    // Product(@Token, Nat) — @Token is concrete after stripping @
    let ty = Type::Product(Box::new(Type::TyVar("@Token".into())), Box::new(Type::Nat));
    assert!(
        !codegen.has_mono_blocking_tyvar(&ty),
        "Product(@Token, Nat) should be fully resolved — @Token is a concrete record"
    );

    // Product(T, Nat) — T is an abstract type variable
    let ty_abstract = Type::Product(Box::new(Type::TyVar("T".into())), Box::new(Type::Nat));
    assert!(
        codegen.has_mono_blocking_tyvar(&ty_abstract),
        "Product(T, Nat) should be unresolved — T is not a known type"
    );
}

/// AC 7: Owner unit emits `define`, non-owner emits `declare` — grep emitted IR.
///
/// Simulates two codegen units sharing a monomorphized `f<Nat>`. The owner
/// unit compiles the define via `compile_monomorphized_named`; the consumer
/// unit only calls `declare_def`. The test greps both IR strings to verify
/// exactly one `define` and one `declare` for the symbol.
#[test]
fn test_no_duplicate_define_across_units_ir() {
    let mono_symbol = "_tg_m_f_I_Nat";

    // ── Owner unit ──────────────────────────────────────────────
    let owner_ctx = Context::create();
    let mut owner = CodeGen::new(&owner_ctx, "owner_unit");

    // Register poly definition
    owner.defs.def_types.insert(
        "f".to_string(),
        Type::Forall("T".to_string(), Box::new(Type::TyVar("T".to_string()))),
    );
    owner.defs.term_defs.insert(
        "f".to_string(),
        Term::TyAbs("T".to_string(), Box::new(Term::NatLit(42))),
    );

    // Compile the monomorphized define
    owner
        .compile_monomorphized_named("f", &[Type::Nat], mono_symbol)
        .expect("owner should compile mono define");

    let owner_ir = owner.get_ir_string();

    // ── Consumer unit ───────────────────────────────────────────
    let consumer_ctx = Context::create();
    let mut consumer = CodeGen::new(&consumer_ctx, "consumer_unit");

    // Only declare (non-owner path)
    consumer
        .declare_def(mono_symbol, &Type::Nat)
        .expect("consumer should declare mono symbol");

    let consumer_ir = consumer.get_ir_string();

    // ── Verify IR ───────────────────────────────────────────────
    let define_pattern = format!("define ");
    let declare_pattern = format!("declare ");

    let owner_defines: Vec<_> = owner_ir
        .lines()
        .filter(|l| l.contains(&define_pattern) && l.contains(mono_symbol))
        .collect();
    let owner_declares: Vec<_> = owner_ir
        .lines()
        .filter(|l| l.contains(&declare_pattern) && l.contains(mono_symbol))
        .collect();

    let consumer_defines: Vec<_> = consumer_ir
        .lines()
        .filter(|l| l.contains(&define_pattern) && l.contains(mono_symbol))
        .collect();
    let consumer_declares: Vec<_> = consumer_ir
        .lines()
        .filter(|l| l.contains(&declare_pattern) && l.contains(mono_symbol))
        .collect();

    // Owner has define, consumer has declare — no duplicates
    assert!(
        !owner_defines.is_empty(),
        "owner IR should contain 'define' for {}: {}",
        mono_symbol,
        owner_ir
    );
    assert!(
        consumer_defines.is_empty(),
        "consumer IR must NOT contain 'define' for {}: {:?}",
        mono_symbol,
        consumer_defines
    );
    assert!(
        !consumer_declares.is_empty(),
        "consumer IR should contain 'declare' for {}: {}",
        mono_symbol,
        consumer_ir
    );
    // Owner should not also declare the same symbol it defines
    // (declare_def is called by compile_monomorphized_named internally,
    // but LLVM promotes it to define, so no standalone declare remains)
    assert!(
        owner_declares.is_empty(),
        "owner IR should not have standalone 'declare' for {}: {:?}",
        mono_symbol,
        owner_declares
    );
}
