#[cfg(test)]
mod self_test_tests {
    use crate::doctor::self_test::Tier;
    #[test]
    fn tier_default_is_not_full() {
        assert_ne!(Tier::Default, Tier::Full);
    }

    #[test]
    fn tier_equality() {
        assert_eq!(Tier::Default, Tier::Default);
        assert_eq!(Tier::Full, Tier::Full);
    }
}

#[cfg(test)]
mod type_graph_tests {
    use crate::doctor::audit_mutual_types::type_graph::TypeGraph;
    use crate::driver::AdtTypes;
    use crate::elaborate::Constructor;
    use tungsten_core::Type;

    fn make_adt(
        name: &str,
        params: Vec<&str>,
        ctors: Vec<(&str, Vec<Type>)>,
    ) -> (String, (Vec<String>, Vec<Constructor>)) {
        let constructors: Vec<Constructor> = ctors
            .into_iter()
            .enumerate()
            .map(|(i, (ctor_name, fields))| Constructor {
                name: ctor_name.to_string(),
                fields,
                index: i,
                visibility: None,
                span: Default::default(),
            })
            .collect();
        (
            name.to_string(),
            (params.into_iter().map(String::from).collect(), constructors),
        )
    }

    #[test]
    fn test_self_recursive_type() {
        let mut adt_types: AdtTypes = std::collections::HashMap::new();
        let (k, v) = make_adt(
            "List",
            vec!["T"],
            vec![
                ("Nil", vec![]),
                (
                    "Cons",
                    vec![
                        Type::TyVar("T".to_string()),
                        Type::App("List".to_string(), vec![Type::TyVar("T".to_string())]),
                    ],
                ),
            ],
        );
        adt_types.insert(k, v);

        let graph = TypeGraph::build_adt_only(&adt_types);
        assert_eq!(graph.node_count(), 1);
        assert!(graph.has_edge("List", "List"));
    }

    #[test]
    fn test_mutual_recursion() {
        let mut adt_types: AdtTypes = std::collections::HashMap::new();
        let (k, v) = make_adt(
            "TypeExpr",
            vec![],
            vec![("TyEq", vec![Type::TyVar("@Expr".to_string())])],
        );
        adt_types.insert(k, v);
        let (k, v) = make_adt(
            "Expr",
            vec![],
            vec![("ExprAnnot", vec![Type::TyVar("@TypeExpr".to_string())])],
        );
        adt_types.insert(k, v);

        let graph = TypeGraph::build_adt_only(&adt_types);
        assert_eq!(graph.node_count(), 2);
        assert!(graph.has_edge("TypeExpr", "Expr"));
        assert!(graph.has_edge("Expr", "TypeExpr"));
    }

    #[test]
    fn test_non_recursive_type() {
        let mut adt_types: AdtTypes = std::collections::HashMap::new();
        let (k, v) = make_adt(
            "Color",
            vec![],
            vec![("Red", vec![]), ("Green", vec![]), ("Blue", vec![])],
        );
        adt_types.insert(k, v);

        let graph = TypeGraph::build_adt_only(&adt_types);
        assert_eq!(graph.node_count(), 1);
        assert!(!graph.has_edge("Color", "Color"));
    }
}
