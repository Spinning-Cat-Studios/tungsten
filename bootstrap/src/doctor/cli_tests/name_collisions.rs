//! The `doctor check module name-collisions` surface: paths, flags and defaults.

/// CLI parsing for the paths ADR 13.8.26c added or moved.
///
/// Separate from `cli_grouping_tests` because these assert *resolved values*,
/// not merely that a spelling parses: every other test of this check passes its
/// inputs in directly, so nothing else exercises the path the binary takes when
/// the user passes only a file.
#[cfg(test)]
mod name_collision_cli_tests {
    use crate::doctor::checks::check_name_collisions::census::{ReexportHandling, Severity};
    use crate::doctor::checks::check_name_collisions::reexport_handling;
    use crate::doctor::{CheckCommands, CheckModuleCommands};
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: CheckCommands,
    }

    fn parse(args: &[&str]) -> Result<TestCli, clap::Error> {
        TestCli::try_parse_from(std::iter::once("test").chain(args.iter().copied()))
    }

    /// The defaults a user gets from `name-collisions <file>` alone: every
    /// class, the human report, and the re-export subtraction ON. The last is
    /// the one that matters — a default flipped the other way turns a clean
    /// corpus into 1126 findings, and no test that passes its inputs in could
    /// notice.
    #[test]
    fn the_bare_invocation_resolves_to_all_classes_and_subtracted_reexports() {
        let cli = parse(&["module", "name-collisions", "main.tg"]).expect("parses");
        let CheckCommands::Module(CheckModuleCommands::NameCollisions {
            file,
            severity,
            json,
            include_reexports,
        }) = cli.cmd
        else {
            panic!("parsed as the wrong subcommand");
        };

        assert_eq!(file.to_string_lossy(), "main.tg");
        assert_eq!(severity, Severity::All);
        assert!(!json, "the human report is the default");
        assert!(!include_reexports, "the subtraction is ON by default");
        assert_eq!(
            reexport_handling(include_reexports),
            ReexportHandling::Subtract,
            "and that flag resolves to the subtracting arm"
        );
    }

    /// `--include-reexports` is the AC-1 measurement arm and must reach the
    /// other `ReexportHandling` value, or the two runs would be the same run.
    #[test]
    fn include_reexports_reaches_the_measurement_arm() {
        let cli = parse(&[
            "module",
            "name-collisions",
            "main.tg",
            "--include-reexports",
        ])
        .expect("parses");
        let CheckCommands::Module(CheckModuleCommands::NameCollisions {
            include_reexports, ..
        }) = cli.cmd
        else {
            panic!("parsed as the wrong subcommand");
        };
        assert!(include_reexports);
        assert_eq!(reexport_handling(include_reexports), ReexportHandling::Keep);
    }

    /// `--severity` accepts the two documented spellings and nothing else.
    #[test]
    fn severity_accepts_live_and_all_and_rejects_anything_else() {
        for (arg, expected) in [("live", Severity::Live), ("all", Severity::All)] {
            let cli = parse(&["module", "name-collisions", "main.tg", "--severity", arg])
                .expect("parses");
            let CheckCommands::Module(CheckModuleCommands::NameCollisions { severity, .. }) =
                cli.cmd
            else {
                panic!("parsed as the wrong subcommand");
            };
            assert_eq!(severity, expected, "--severity {arg}");
        }
        assert!(
            parse(&["module", "name-collisions", "main.tg", "--severity", "loud"]).is_err(),
            "an undocumented severity is a usage error, not a silent default"
        );
    }

    /// The file is required. A default would be a path to point at the wrong
    /// place, and `0 collisions` would then read as clean (ADR 11.8.26c).
    #[test]
    fn the_file_is_required() {
        assert!(parse(&["module", "name-collisions"]).is_err());
    }

    /// ADR 29.8.26a D2: the hidden alias resolves to the SAME values as the
    /// grouped spelling. Parsing is not enough — an alias whose `--severity`
    /// defaulted the other way would keep every caller running and quietly
    /// change what they were told.
    #[test]
    fn the_flat_alias_resolves_to_the_same_defaults() {
        let cli = parse(&["name-collisions", "main.tg"]).expect("the alias still parses");
        let CheckCommands::NameCollisionsLegacy {
            file,
            severity,
            json,
            include_reexports,
        } = cli.cmd
        else {
            panic!("parsed as the wrong subcommand");
        };
        assert_eq!(file.to_string_lossy(), "main.tg");
        assert_eq!(severity, Severity::All);
        assert!(!json);
        assert!(!include_reexports);
    }

    /// D5's move: the grouped spellings work AND the flat ones still do, so no
    /// existing caller breaks.
    #[test]
    fn the_link_subnamespace_and_its_hidden_aliases_both_parse() {
        assert!(parse(&["link", "health", "./tungsten1"]).is_ok());
        assert!(
            parse(&["link-health", "./tungsten1"]).is_ok(),
            "the flat spelling is a hidden alias, not a removal"
        );
        #[cfg(feature = "codegen")]
        {
            assert!(parse(&["link", "collisions", "/tmp/objs"]).is_ok());
            assert!(parse(&["link-collisions", "/tmp/objs"]).is_ok());
        }
    }
}
