//! Every regrouping keeps its flat spelling working, flags and all.
//!
//! One table drives both assertions, so a group added without its alias — or
//! with an alias that silently dropped a flag — fails here rather than in
//! whichever caller still spells it the old way.

/// Grouped spellings and their hidden flat aliases, for the two sub-namespaces
/// ADR 13.8.26c's review introduced.
///
/// The aliases exist so no caller breaks; a test is what keeps that true, since
/// a hidden variant appears in no `--help` a human reads and its removal would
/// surface only as someone's script failing.
#[cfg(test)]
mod grouped_and_alias_spellings {
    use crate::doctor::CheckCommands;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: CheckCommands,
    }

    fn parses(args: &[&str]) -> bool {
        TestCli::try_parse_from(std::iter::once("test").chain(args.iter().copied())).is_ok()
    }

    /// Every pair: the grouped path AND the flat alias it replaced.
    #[test]
    fn both_spellings_parse_for_every_regrouped_check() {
        let pairs: &[(&[&str], &[&str])] = &[
            (&["link", "health", "./bin"], &["link-health", "./bin"]),
            (
                &["type", "determinism", "normalization", "main.tg"],
                &["type", "normalization-consistency", "main.tg"],
            ),
            (
                &["type", "determinism", "encoding", "main.tg"],
                &["type", "encoding-determinism", "main.tg"],
            ),
            (
                &["type", "determinism", "resolution-attempts", "main.tg"],
                &["type", "resolution-attempt-determinism", "main.tg"],
            ),
            // `integrity type-stubs` is the one pair whose alias is also a
            // RENAME (`stubs`), so a regression here reads as "command not
            // found" rather than as a moved path.
            (
                &["type", "integrity", "type-stubs", "main.tg"],
                &["type", "stubs", "main.tg"],
            ),
            (
                &["type", "integrity", "constructor-stubs", "main.tg"],
                &["type", "constructor-stubs", "main.tg"],
            ),
            (
                &["type", "integrity", "constructor-counts", "main.tg"],
                &["type", "constructor-counts", "main.tg"],
            ),
            (
                &["type", "integrity", "phase-invariants", "main.tg"],
                &["type", "phase-invariants", "main.tg"],
            ),
            // The `check`-level flat spellings predate the `check type`
            // grouping and are a second alias layer; ADR 15.8.26a did not
            // touch them, and this pins that they still resolve.
            (
                &["type", "integrity", "type-stubs", "main.tg"],
                &["stubs", "main.tg"],
            ),
            (
                &["type", "integrity", "constructor-counts", "main.tg"],
                &["constructor-counts", "main.tg"],
            ),
            (
                &["type", "integrity", "phase-invariants", "main.tg"],
                &["phase-invariants", "main.tg"],
            ),
            #[cfg(feature = "codegen")]
            (
                &["codegen", "unit-cost", "main.tg"],
                &["unit-cost", "main.tg"],
            ),
            #[cfg(feature = "codegen")]
            (
                &["codegen", "tco-coverage", "main.tg"],
                &["tco-coverage", "main.tg"],
            ),
            #[cfg(feature = "codegen")]
            (
                &["codegen", "mono-coverage", "main.tg"],
                &["mono-coverage", "main.tg"],
            ),
            #[cfg(feature = "codegen")]
            (
                &["codegen", "extern-map-ambiguity", "main.tg"],
                &["extern-map-ambiguity", "main.tg"],
            ),
            #[cfg(feature = "codegen")]
            (
                &["link", "collisions", "/tmp/objs"],
                &["link-collisions", "/tmp/objs"],
            ),
            // ADR 29.8.26a D2 — the four module-and-name checks. `overlap` is
            // the one pair whose alias is also a RENAME (`module-overlap`
            // loses the prefix the namespace now carries), so a regression
            // there reads as "command not found" rather than as a moved path.
            (
                &["module", "reexport-completeness", "main.tg"],
                &["reexport-completeness", "main.tg"],
            ),
            (
                &["module", "name-collisions", "main.tg"],
                &["name-collisions", "main.tg"],
            ),
            (&["module", "overlap"], &["module-overlap"]),
            (
                &["module", "signature-collection", "main.tg"],
                &["signature-collection", "main.tg"],
            ),
        ];

        for (grouped, alias) in pairs {
            assert!(parses(grouped), "grouped spelling {grouped:?}");
            assert!(parses(alias), "hidden alias {alias:?}");
        }
    }

    /// The flags survive the move. A regrouping that silently dropped one would
    /// leave both spellings parsing and one of them useless.
    #[test]
    fn the_regrouped_checks_keep_their_flags_under_both_spellings() {
        for args in [
            &[
                "type",
                "determinism",
                "normalization",
                "main.tg",
                "--raw-only",
            ][..],
            &["type", "normalization-consistency", "main.tg", "--raw-only"][..],
            &["type", "determinism", "encoding", "main.tg", "--json"][..],
            &["type", "encoding-determinism", "main.tg", "--json"][..],
            &[
                "type",
                "determinism",
                "resolution-attempts",
                "main.tg",
                "--json",
            ][..],
            &[
                "type",
                "resolution-attempt-determinism",
                "main.tg",
                "--json",
            ][..],
            &[
                "type",
                "integrity",
                "constructor-counts",
                "main.tg",
                "--json",
            ][..],
            &["type", "constructor-counts", "main.tg", "--json"][..],
            &["constructor-counts", "main.tg", "--json"][..],
            // ADR 29.8.26a: `name-collisions` carries three flags and
            // `overlap` two, and an alias that quietly dropped one would leave
            // both spellings parsing with only one of them usable.
            &[
                "module",
                "name-collisions",
                "main.tg",
                "--severity",
                "live",
                "--json",
                "--include-reexports",
            ][..],
            &[
                "name-collisions",
                "main.tg",
                "--severity",
                "live",
                "--json",
                "--include-reexports",
            ][..],
            &["module", "overlap", "--path", "bootstrap/src", "--json"][..],
            &["module-overlap", "--path", "bootstrap/src", "--json"][..],
        ] {
            assert!(parses(args), "{args:?}");
        }

        #[cfg(feature = "codegen")]
        for args in [
            &["codegen", "tco-coverage", "main.tg", "--gate"][..],
            &["tco-coverage", "main.tg", "--gate"][..],
            &["codegen", "unit-cost", "main.tg", "--emit-serial-list"][..],
            &["unit-cost", "main.tg", "--emit-serial-list"][..],
            &["codegen", "unit-cost", "main.tg", "--threshold", "0.5s"][..],
            &["unit-cost", "main.tg", "--threshold", "0.5s"][..],
        ] {
            assert!(parses(args), "{args:?}");
        }
    }
}
