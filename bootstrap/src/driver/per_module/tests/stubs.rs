use super::*;

#[test]
fn phase_a_collects_type_stubs() {
    let items = vec![Item::TypeDef(crate::ast::TypeDef {
        visibility: crate::ast::Visibility::Public,
        name: crate::ast::Ident {
            name: "MyType".to_string(),
            span: crate::span::Span::new(0, 0),
        },
        type_params: vec![],
        body: crate::ast::TypeBody::Sum(vec![
            crate::ast::Variant {
                visibility: None,
                name: crate::ast::Ident {
                    name: "A".to_string(),
                    span: crate::span::Span::new(0, 0),
                },
                fields: vec![],
                span: crate::span::Span::new(0, 0),
            },
            crate::ast::Variant {
                visibility: None,
                name: crate::ast::Ident {
                    name: "B".to_string(),
                    span: crate::span::Span::new(0, 0),
                },
                fields: vec![crate::ast::Field {
                    name: None,
                    ty: crate::ast::TypeExpr::Path(crate::ast::Path::simple(crate::ast::Ident {
                        name: "Nat".to_string(),
                        span: crate::span::Span::new(0, 0),
                    })),
                    span: crate::span::Span::new(0, 0),
                }],
                span: crate::span::Span::new(0, 0),
            },
        ]),
        span: crate::span::Span::new(0, 0),
    })];

    let module = make_parsed_module(items);
    let mut exports = ModuleExports::default();
    collect_all_type_and_constructor_stubs(&module, &mut exports);

    // Should have 1 type stub (with ADT shape, not bare Stub)
    assert_eq!(exports.types.len(), 1);
    assert_eq!(exports.types[0].0, "MyType");
    assert!(matches!(exports.types[0].1.kind, TypeDefKind::ADT(_)));
    if let TypeDefKind::ADT(ctors) = &exports.types[0].1.kind {
        assert_eq!(ctors.len(), 2);
        assert_eq!(ctors[0].name, "A");
        assert_eq!(ctors[0].fields.len(), 0);
        assert_eq!(ctors[1].name, "B");
        assert_eq!(ctors[1].fields.len(), 1);
    }

    // Should have 2 constructor stubs
    assert_eq!(exports.constructors.len(), 2);
    assert_eq!(exports.constructors[0].0, "A");
    assert_eq!(exports.constructors[0].1.arity, 0);
    assert_eq!(exports.constructors[1].0, "B");
    assert_eq!(exports.constructors[1].1.arity, 1);
}

#[test]
fn phase_a_recurses_into_submodules() {
    let child = ParsedModule {
        path: std::path::PathBuf::from("child.tg"),
        source_file: crate::ast::SourceFile {
            items: vec![Item::TypeAlias(crate::ast::TypeAlias {
                visibility: crate::ast::Visibility::Public,
                name: crate::ast::Ident {
                    name: "ChildType".to_string(),
                    span: crate::span::Span::new(0, 0),
                },
                type_params: vec![crate::ast::TypeParam {
                    name: crate::ast::Ident {
                        name: "T".to_string(),
                        span: crate::span::Span::new(0, 0),
                    },
                    bounds: vec![],
                    span: crate::span::Span::new(0, 0),
                }],
                ty: crate::ast::TypeExpr::Path(crate::ast::Path::simple(crate::ast::Ident {
                    name: "T".to_string(),
                    span: crate::span::Span::new(0, 0),
                })),
                span: crate::span::Span::new(0, 0),
            })],
            span: crate::span::Span::new(0, 0),
        },
        submodules: vec![],
        visibility: crate::ast::Visibility::Public,
    };

    let parent = ParsedModule {
        path: std::path::PathBuf::from("parent.tg"),
        source_file: crate::ast::SourceFile {
            items: vec![],
            span: crate::span::Span::new(0, 0),
        },
        submodules: vec![child],
        visibility: crate::ast::Visibility::Public,
    };

    let mut exports = ModuleExports::default();
    collect_all_type_and_constructor_stubs(&parent, &mut exports);

    assert_eq!(exports.types.len(), 1);
    assert_eq!(exports.types[0].0, "ChildType");
    assert_eq!(exports.types[0].1.params, vec!["T".to_string()]);
}

#[test]
fn merge_exports_overwrites_stubs() {
    let mut acc = ModuleTreeAccumulator::new();

    // Phase A: add a stub with placeholder constructors
    acc.exports.types.push((
        "Foo".to_string(),
        TypeDef {
            name: "Foo".to_string(),
            params: vec![],
            kind: TypeDefKind::ADT(vec![Constructor {
                name: "Bar".to_string(),
                fields: vec![Type::Unit], // placeholder
                index: 0,
                visibility: None,
                span: crate::span::Span::new(0, 0),
            }]),
            visibility: crate::ast::Visibility::Public,
            span: crate::span::Span::new(0, 0),
            defining_module: None,
            encoded_type: None,
            field_visibilities: Vec::new(),
        },
    ));

    // Phase B: merge real type with proper field types
    let real_exports = ModuleExports {
        types: vec![(
            "Foo".to_string(),
            TypeDef {
                name: "Foo".to_string(),
                params: vec![],
                kind: TypeDefKind::ADT(vec![Constructor {
                    name: "Bar".to_string(),
                    fields: vec![Type::Nat], // real type
                    index: 0,
                    visibility: None,
                    span: crate::span::Span::new(0, 0),
                }]),
                visibility: crate::ast::Visibility::Public,
                span: crate::span::Span::new(0, 0),
                defining_module: None,
                encoded_type: Some(Type::Nat), // has encoding
                field_visibilities: Vec::new(),
            },
        )],
        values: vec![],
        constructors: vec![],
    };

    acc.merge_exports(real_exports);

    // Real type should replace Phase A stub
    assert_eq!(acc.exports.types.len(), 1);
    assert!(matches!(acc.exports.types[0].1.kind, TypeDefKind::ADT(_)));
    // Check it's the real one (has encoded_type)
    assert!(acc.exports.types[0].1.encoded_type.is_some());
}

#[test]
fn phase_a_alias_resolves_builtins() {
    let make_alias = |name: &str, target: &str| {
        Item::TypeAlias(crate::ast::TypeAlias {
            visibility: crate::ast::Visibility::Public,
            name: crate::ast::Ident {
                name: name.to_string(),
                span: crate::span::Span::new(0, 0),
            },
            type_params: vec![],
            ty: crate::ast::TypeExpr::Path(crate::ast::Path::simple(crate::ast::Ident {
                name: target.to_string(),
                span: crate::span::Span::new(0, 0),
            })),
            span: crate::span::Span::new(0, 0),
        })
    };

    let module = make_parsed_module(vec![
        make_alias("MyNat", "Nat"),
        make_alias("MyBool", "Bool"),
        make_alias("MyString", "String"),
    ]);

    let mut exports = ModuleExports::default();
    collect_all_type_and_constructor_stubs(&module, &mut exports);

    assert_eq!(exports.types.len(), 3);
    assert!(matches!(
        exports.types[0].1.kind,
        TypeDefKind::Alias(Type::Nat)
    ));
    assert!(matches!(
        exports.types[1].1.kind,
        TypeDefKind::Alias(Type::Bool)
    ));
    assert!(matches!(
        exports.types[2].1.kind,
        TypeDefKind::Alias(Type::String)
    ));
}

#[test]
fn phase_a_alias_complex_stays_stub() {
    // Parameterized alias: `type Wrapper<T> = T` → should get TypeDefKind::Stub
    let module = make_parsed_module(vec![Item::TypeAlias(crate::ast::TypeAlias {
        visibility: crate::ast::Visibility::Public,
        name: crate::ast::Ident {
            name: "Wrapper".to_string(),
            span: crate::span::Span::new(0, 0),
        },
        type_params: vec![crate::ast::TypeParam {
            name: crate::ast::Ident {
                name: "T".to_string(),
                span: crate::span::Span::new(0, 0),
            },
            bounds: vec![],
            span: crate::span::Span::new(0, 0),
        }],
        ty: crate::ast::TypeExpr::Path(crate::ast::Path::simple(crate::ast::Ident {
            name: "T".to_string(),
            span: crate::span::Span::new(0, 0),
        })),
        span: crate::span::Span::new(0, 0),
    })]);

    let mut exports = ModuleExports::default();
    collect_all_type_and_constructor_stubs(&module, &mut exports);

    assert_eq!(exports.types.len(), 1);
    assert_eq!(exports.types[0].0, "Wrapper");
    assert!(matches!(exports.types[0].1.kind, TypeDefKind::Stub));
}

#[test]
fn merge_exports_overwrites_values() {
    let mut acc = ModuleTreeAccumulator::new();

    // Phase A.5: pre-register a value stub
    acc.exports.values.push((
        "my_fn".to_string(),
        ValueDef {
            name: "my_fn".to_string(),
            ty: Type::Unit, // placeholder type
            visibility: crate::ast::Visibility::Public,
            span: crate::span::Span::new(0, 0),
        },
    ));

    // Phase B: merge real value with proper type
    let real_exports = ModuleExports {
        types: vec![],
        values: vec![(
            "my_fn".to_string(),
            ValueDef {
                name: "my_fn".to_string(),
                ty: Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool)),
                visibility: crate::ast::Visibility::Public,
                span: crate::span::Span::new(0, 0),
            },
        )],
        constructors: vec![],
    };

    acc.merge_exports(real_exports);

    // Real value should replace Phase A.5 stub
    assert_eq!(acc.exports.values.len(), 1);
    assert_eq!(acc.exports.values[0].0, "my_fn");
    assert!(matches!(acc.exports.values[0].1.ty, Type::Arrow(_, _)));
}
