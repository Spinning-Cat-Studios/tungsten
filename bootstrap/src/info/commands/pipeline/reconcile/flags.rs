//! Which long flags a clap command accepts, and which one a usage line names —
//! the two halves of the `Flag` reconciliation (ADR 28.7.26f D3).

use std::collections::HashSet;

use clap::CommandFactory;

use crate::cli::Cli;

/// The bare long name a usage line documents, or `None` if it names no flag.
///
/// Usage strings carry display decoration that is not part of the flag:
/// `--trace-types=<name>`, `--trace-encoding[=name]`, `--json (check only)`.
pub fn documented_flag_name(usage: &str) -> Option<&str> {
    let rest = usage.strip_prefix("--")?;
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .unwrap_or(rest.len());
    (end > 0).then(|| &usage[..2 + end])
}

/// Long option names the command at `path` accepts, or `None` when that command
/// does not exist in this build's tree. `""` addresses the root command.
///
/// Unions the command's own arguments with the root's, because clap declares
/// global arguments (`--hints`, `--verbose`, …) once on the root and propagates
/// them to every subcommand at parse time.
pub fn accepted_long_flags(path: &str) -> Option<HashSet<String>> {
    let root = Cli::command();
    let mut command = &root;
    for name in path.split_whitespace() {
        command = command.find_subcommand(name)?;
    }
    let longs = |c: &clap::Command| -> Vec<String> {
        c.get_arguments()
            .filter_map(|a| a.get_long().map(str::to_string))
            .collect()
    };
    let mut accepted: HashSet<String> = longs(command).into_iter().collect();
    accepted.extend(longs(&root));
    Some(accepted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_flag_is_its_own_name() {
        assert_eq!(documented_flag_name("--emit-llvm"), Some("--emit-llvm"));
    }

    #[test]
    fn an_equals_placeholder_is_display_decoration() {
        assert_eq!(
            documented_flag_name("--trace-types=<name>"),
            Some("--trace-types")
        );
    }

    #[test]
    fn an_optional_value_bracket_is_display_decoration() {
        assert_eq!(
            documented_flag_name("--trace-encoding[=name]"),
            Some("--trace-encoding")
        );
    }

    #[test]
    fn a_trailing_parenthetical_is_display_decoration() {
        assert_eq!(documented_flag_name("--json (check only)"), Some("--json"));
    }

    #[test]
    fn a_usage_that_names_no_flag_yields_nothing() {
        assert_eq!(documented_flag_name("make check-ir-audits"), None);
        assert_eq!(documented_flag_name("tungsten info def <n>"), None);
        // A bare `--` is a separator, not a flag; without the length guard it
        // would parse as the empty flag name and match nothing forever.
        assert_eq!(documented_flag_name("--"), None);
    }

    #[test]
    fn the_root_command_carries_the_global_flags() {
        let root = accepted_long_flags("").expect("the root always resolves");
        assert!(root.contains("hints"), "{root:?}");
        assert!(root.contains("no-hints"), "{root:?}");
    }

    #[test]
    fn a_subcommand_sees_its_own_flags_and_the_globals() {
        let check = accepted_long_flags("check").expect("`check` is not feature-gated");
        assert!(check.contains("json"), "own flag missing: {check:?}");
        assert!(check.contains("hints"), "global not propagated: {check:?}");
    }

    #[test]
    fn a_nested_path_resolves_through_the_namespaces() {
        assert!(accepted_long_flags("doctor check ir declares").is_some());
    }

    #[test]
    fn an_unknown_command_resolves_to_nothing() {
        assert!(accepted_long_flags("no-such-command").is_none());
        assert!(accepted_long_flags("doctor no-such-check").is_none());
    }
}
