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

        let graph = TypeGraph::build(&adt_types);
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

        let graph = TypeGraph::build(&adt_types);
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

        let graph = TypeGraph::build(&adt_types);
        assert_eq!(graph.node_count(), 1);
        assert!(!graph.has_edge("Color", "Color"));
    }
}

/// CLI grouping tests — verify both grouped and legacy check paths parse (ADR 12.5.26h).
#[cfg(test)]
mod cli_grouping_tests {
    use crate::doctor::CheckCommands;
    use clap::Parser;
    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: CheckCommands,
    }

    fn parse(args: &[&str]) -> Result<TestCli, clap::Error> {
        TestCli::try_parse_from(std::iter::once("test").chain(args.iter().copied()))
    }

    // ── Grouped type paths ──

    #[test]
    fn test_check_type_normalization_consistency_parses() {
        assert!(parse(&["type", "normalization-consistency", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_encoding_depth_parses() {
        assert!(parse(&["type", "encoding-depth", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_encoding_depth_with_thresholds_parses() {
        assert!(parse(&[
            "type",
            "encoding-depth",
            "test.tg",
            "--max-stack",
            "10",
            "--max-depth",
            "30",
            "--max-nodes",
            "2000"
        ])
        .is_ok());
    }

    #[test]
    fn test_check_type_phase_invariants_parses() {
        assert!(parse(&["type", "phase-invariants", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_type_sizes_parses() {
        assert!(parse(&["type", "type-sizes", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_stubs_parses() {
        assert!(parse(&["type", "stubs", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_fold_consistency_parses() {
        assert!(parse(&["type", "fold-consistency", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_constructor_counts_parses() {
        assert!(parse(&["type", "constructor-counts", "test.tg"]).is_ok());
    }

    // ── Grouped IR paths ──

    #[test]
    fn test_check_ir_layout_parses() {
        assert!(parse(&["ir", "layout", "test.ll"]).is_ok());
    }

    #[test]
    fn test_check_ir_declares_parses() {
        assert!(parse(&["ir", "declares", "--from-existing-ir", "target/ll/"]).is_ok());
    }

    // ── Legacy paths (hidden aliases) ──

    #[test]
    fn test_check_normalization_consistency_legacy_parses() {
        assert!(parse(&["normalization-consistency", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_encoding_depth_legacy_parses() {
        assert!(parse(&["encoding-depth", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_stubs_legacy_parses() {
        assert!(parse(&["stubs", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_phase_invariants_legacy_parses() {
        assert!(parse(&["phase-invariants", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_sizes_legacy_parses() {
        assert!(parse(&["type-sizes", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_fold_consistency_legacy_parses() {
        assert!(parse(&["fold-consistency", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_constructor_counts_legacy_parses() {
        assert!(parse(&["constructor-counts", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_ir_layout_legacy_parses() {
        assert!(parse(&["ir-layout", "test.ll"]).is_ok());
    }

    #[test]
    fn test_check_declares_legacy_parses() {
        assert!(parse(&["declares", "--from-existing-ir", "target/ll/"]).is_ok());
    }

    // ── Top-level (unchanged) ──

    #[test]
    fn test_check_module_overlap_parses() {
        assert!(parse(&["module-overlap"]).is_ok());
    }

    #[test]
    fn test_check_reexport_completeness_parses() {
        assert!(parse(&["reexport-completeness", "test.tg"]).is_ok());
    }
}
