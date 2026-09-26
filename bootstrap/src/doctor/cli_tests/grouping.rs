//! Which `doctor check` spellings clap accepts, sub-namespace by sub-namespace.

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
    fn test_check_type_determinism_normalization_parses() {
        assert!(parse(&["type", "determinism", "normalization", "test.tg"]).is_ok());
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
    fn test_check_type_integrity_phase_invariants_parses() {
        assert!(parse(&["type", "integrity", "phase-invariants", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_type_sizes_parses() {
        assert!(parse(&["type", "type-sizes", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_integrity_type_stubs_parses() {
        assert!(parse(&["type", "integrity", "type-stubs", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_fold_consistency_parses() {
        assert!(parse(&["type", "fold-consistency", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_integrity_constructor_counts_parses() {
        assert!(parse(&["type", "integrity", "constructor-counts", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_type_integrity_constructor_stubs_parses() {
        assert!(parse(&["type", "integrity", "constructor-stubs", "test.tg"]).is_ok());
    }

    // ── unit-cost census (ADR 8.7.26a) ──

    #[cfg(feature = "codegen")]
    #[test]
    fn test_check_unit_cost_parses() {
        assert!(parse(&["unit-cost", "test.tg"]).is_ok());
        assert!(parse(&["unit-cost", "test.tg", "--json"]).is_ok());
        assert!(parse(&["unit-cost", "test.tg", "--threshold", "0.5s"]).is_ok());
        assert!(parse(&["unit-cost", "test.tg", "--threshold", "8GB", "--json"]).is_ok());
        assert!(parse(&["unit-cost", "test.tg", "--emit-serial-list"]).is_ok());
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

    #[test]
    fn test_check_ir_wrapper_self_calls_parses() {
        assert!(parse(&["ir", "wrapper-self-calls", "target/ll/"]).is_ok());
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

    // ── Grouped module paths (ADR 29.8.26a D1) ──

    #[test]
    fn test_check_module_overlap_parses() {
        assert!(parse(&["module", "overlap"]).is_ok());
    }

    #[test]
    fn test_check_module_reexport_completeness_parses() {
        assert!(parse(&["module", "reexport-completeness", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_module_name_collisions_parses() {
        assert!(parse(&["module", "name-collisions", "test.tg"]).is_ok());
    }

    #[test]
    fn test_check_module_signature_collection_parses() {
        assert!(parse(&["module", "signature-collection", "test.tg"]).is_ok());
    }

    /// The namespace is not a command of its own, and neither is a member
    /// spelled without it. Both would leave `module` looking like it had
    /// absorbed the old flat surface rather than grouped it.
    #[test]
    fn test_check_module_rejects_a_non_member() {
        assert!(parse(&["module", "comparable", "List", "test.tg"]).is_err());
        assert!(parse(&["module", "no-such-check"]).is_err());
    }

    /// ADR 29.8.26a D4/AC 2: `doctor check` offers at most ten commands —
    /// eleven since ADR 18.9.26g spent one slot on `sorry-sites`, a property of
    /// a *term*, which no existing group is about (that ADR's Decision 1).
    ///
    /// **Read the exclusions before trusting the number.** `cli-surface` counts
    /// the source enum's non-`hide = true` variants and this counts what clap
    /// built, and the two disagree in both directions: clap synthesises a `help`
    /// row that is not a check, and a `--no-default-features` build has no
    /// `codegen` variant to count. Both are handled, and the *set* is asserted
    /// rather than only its size — a bound alone is satisfied by deleting a
    /// command, which is the move `cli-surface`'s message exists to warn
    /// against.
    #[test]
    fn check_offers_at_most_eleven_commands() {
        use clap::CommandFactory;

        let command = TestCli::command();
        let mut visible: Vec<&str> = command
            .get_subcommands()
            .filter(|sub| !sub.is_hide_set() && sub.get_name() != "help")
            .map(|sub| sub.get_name())
            .collect();
        visible.sort_unstable();

        let mut expected = vec![
            "comparable",
            "extern-coverage",
            "ir",
            "link",
            "module",
            "nested-patterns",
            "self-compile-readiness",
            "selfhost",
            "sorry-sites",
            "type",
        ];
        #[cfg(feature = "codegen")]
        expected.push("codegen");
        expected.sort_unstable();

        assert_eq!(visible, expected);
        assert!(
            visible.len() <= 11,
            "the cap ADR 29.8.26a bought headroom under, less 18.9.26g's slot"
        );
    }
}
