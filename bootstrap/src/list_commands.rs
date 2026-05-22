//! Command listing for `tungsten commands`.
//!
//! Walks the clap `Command` tree to produce flat, tree, and JSON listings
//! of all non-hidden subcommands.

use std::fmt::Write;

/// Flat listing: one line per leaf command with fully-qualified path.
pub fn list_commands_flat(cmd: &clap::Command, prefix: &str) -> String {
    let mut output = String::new();
    list_commands_flat_inner(&mut output, cmd, prefix);
    output
}

fn list_commands_flat_inner(out: &mut String, cmd: &clap::Command, prefix: &str) {
    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() {
            continue;
        }
        let path = if prefix.is_empty() {
            sub.get_name().to_string()
        } else {
            format!("{prefix} {}", sub.get_name())
        };
        let about = sub
            .get_about()
            .map(std::string::ToString::to_string)
            .unwrap_or_default();
        if sub.has_subcommands() && !sub.get_subcommands().all(clap::Command::is_hide_set) {
            list_commands_flat_inner(out, sub, &path);
        } else {
            writeln!(out, "{path:<45} {about}").unwrap();
        }
    }
}

/// Tree listing: hierarchical output with box-drawing characters.
pub fn list_commands_tree(cmd: &clap::Command) -> String {
    let mut output = String::new();
    list_commands_tree_inner(&mut output, cmd, "", true);
    output
}

fn list_commands_tree_inner(out: &mut String, cmd: &clap::Command, indent: &str, is_root: bool) {
    let subs: Vec<_> = cmd.get_subcommands().filter(|s| !s.is_hide_set()).collect();
    for (i, sub) in subs.iter().enumerate() {
        let is_last = i == subs.len() - 1;
        let connector = if is_root {
            ""
        } else if is_last {
            "└── "
        } else {
            "├── "
        };
        let about = sub
            .get_about()
            .map(|s| format!("  — {s}"))
            .unwrap_or_default();
        writeln!(out, "{indent}{connector}{}{about}", sub.get_name()).unwrap();
        if sub.has_subcommands() {
            let child_indent = if is_root {
                "  ".to_string()
            } else if is_last {
                format!("{indent}    ")
            } else {
                format!("{indent}│   ")
            };
            list_commands_tree_inner(out, sub, &child_indent, false);
        }
    }
}

/// JSON listing: array of `{"command": "...", "description": "..."}` objects.
pub fn list_commands_json(cmd: &clap::Command) -> String {
    fn collect(cmd: &clap::Command, prefix: &str) -> Vec<serde_json::Value> {
        let mut entries = Vec::new();
        for sub in cmd.get_subcommands() {
            if sub.is_hide_set() {
                continue;
            }
            let path = if prefix.is_empty() {
                sub.get_name().to_string()
            } else {
                format!("{prefix} {}", sub.get_name())
            };
            let about = sub
                .get_about()
                .map(std::string::ToString::to_string)
                .unwrap_or_default();
            if sub.has_subcommands() && !sub.get_subcommands().all(clap::Command::is_hide_set) {
                entries.extend(collect(sub, &path));
            } else {
                entries.push(serde_json::json!({
                    "command": path,
                    "description": about,
                }));
            }
        }
        entries
    }
    let entries = collect(cmd, "");
    serde_json::to_string_pretty(&entries).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Command;

    /// Build a small command tree for testing.
    fn test_command() -> Command {
        Command::new("test")
            .subcommand(Command::new("check").about("Type-check a file"))
            .subcommand(Command::new("run").about("Run a file"))
            .subcommand(
                Command::new("info")
                    .about("Query information")
                    .subcommand(Command::new("types").about("List types"))
                    .subcommand(Command::new("adt").about("Show ADT details")),
            )
            .subcommand(Command::new("hidden").about("Hidden command").hide(true))
    }

    #[test]
    fn flat_contains_leaf_commands() {
        let cmd = test_command();
        let output = list_commands_flat(&cmd, "");
        assert!(output.contains("check"), "should contain 'check'");
        assert!(output.contains("run"), "should contain 'run'");
        assert!(output.contains("info types"), "should contain 'info types'");
        assert!(output.contains("info adt"), "should contain 'info adt'");
    }

    #[test]
    fn flat_skips_parent_with_visible_subcommands() {
        let cmd = test_command();
        let output = list_commands_flat(&cmd, "");
        // "info" as a namespace should not appear as a standalone leaf
        // — only its children "info types" and "info adt" should appear.
        for line in output.lines() {
            // The command path occupies the first 45 chars (padded format)
            if line.len() >= 45 {
                let cmd_path = line[..45].trim();
                assert_ne!(
                    cmd_path, "info",
                    "parent 'info' should not appear as a standalone leaf"
                );
            }
        }
    }

    #[test]
    fn flat_excludes_hidden_commands() {
        let cmd = test_command();
        let output = list_commands_flat(&cmd, "");
        assert!(
            !output.contains("hidden"),
            "hidden commands should be excluded"
        );
    }

    #[test]
    fn json_is_valid_and_non_empty() {
        let cmd = test_command();
        let output = list_commands_json(&cmd);
        let parsed: Vec<serde_json::Value> =
            serde_json::from_str(&output).expect("should be valid JSON");
        assert!(!parsed.is_empty(), "JSON output should not be empty");
        for entry in &parsed {
            assert!(entry.get("command").is_some(), "entry needs 'command' key");
            assert!(
                entry.get("description").is_some(),
                "entry needs 'description' key"
            );
        }
    }

    #[test]
    fn json_contains_nested_commands() {
        let cmd = test_command();
        let output = list_commands_json(&cmd);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();
        let commands: Vec<&str> = parsed
            .iter()
            .map(|e| e["command"].as_str().unwrap())
            .collect();
        assert!(commands.contains(&"info types"));
        assert!(commands.contains(&"info adt"));
    }

    #[test]
    fn json_excludes_hidden_commands() {
        let cmd = test_command();
        let output = list_commands_json(&cmd);
        assert!(
            !output.contains("hidden"),
            "hidden commands should be excluded from JSON"
        );
    }

    #[test]
    fn tree_contains_hierarchy_characters() {
        let cmd = test_command();
        let output = list_commands_tree(&cmd);
        assert!(
            output.contains("├──") || output.contains("└──"),
            "tree output should use box-drawing characters"
        );
    }

    #[test]
    fn tree_excludes_hidden_commands() {
        let cmd = test_command();
        let output = list_commands_tree(&cmd);
        assert!(
            !output.contains("hidden"),
            "hidden commands should be excluded from tree"
        );
    }

    #[test]
    fn tree_contains_nested_names() {
        let cmd = test_command();
        let output = list_commands_tree(&cmd);
        assert!(output.contains("types"), "tree should contain 'types'");
        assert!(output.contains("adt"), "tree should contain 'adt'");
    }

    #[test]
    fn flat_with_prefix_prepends_path() {
        let cmd = Command::new("test").subcommand(Command::new("check").about("Check it"));
        let output = list_commands_flat(&cmd, "myapp");
        assert!(output.contains("myapp check"), "should prepend prefix");
    }
}
