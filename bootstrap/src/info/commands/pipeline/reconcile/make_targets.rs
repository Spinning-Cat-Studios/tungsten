//! Extracting the repo's `make` target names, for the `MakeTarget` arm of the
//! `info pipeline` reconciliation (ADR 28.7.26f §3/P5).
//!
//! ADR 28.7.26f §3 left this conditional on a cheap, stable extraction existing.
//! It does, and it is deliberately **not** `make help`: shelling out to `make`
//! would need make on `PATH` and would make the test's verdict depend on the
//! environment. A rule header is `name:` at the start of a line, which
//! `Makefile` and `make/*.mk` state directly — no GNU make semantics required,
//! so this cannot be wrong about a target that exists.

use std::collections::HashSet;
use std::path::PathBuf;

/// Target names declared by one makefile's text.
///
/// Recognises a rule header — one or more target names, then `:`, at the start
/// of a line. Skipped: indented lines (recipe bodies, where a `:` is shell
/// syntax), comments, and directives such as `.PHONY:` (a declaration *about*
/// targets, not a definition of one — its operands are already declared by
/// their own rules).
pub fn make_targets_in(text: &str) -> HashSet<String> {
    let mut targets = HashSet::new();
    for line in text.lines() {
        let Some((head, _)) = line.split_once(':') else {
            continue;
        };
        if head.is_empty() || head.starts_with([' ', '\t', '#', '.']) {
            continue;
        }
        for name in head.split_whitespace() {
            targets.insert(name.to_string());
        }
    }
    targets
}

/// Every target name declared by `Makefile` and every `.mk` under `make/`.
pub fn declared_make_targets() -> HashSet<String> {
    let mut targets = HashSet::new();
    for source in makefile_sources() {
        if let Ok(text) = std::fs::read_to_string(&source) {
            targets.extend(make_targets_in(&text));
        }
    }
    targets
}

/// `Makefile` plus every `.mk` under `make/`, **at any depth**.
///
/// The nesting is load-bearing, not defensive: ADR 31.7.26c split `quality.mk`
/// into `make/quality/*.mk` and `devcontainer.mk` into `make/devcontainer/`, and
/// a flat `read_dir` stopped seeing `check-ir-audits` and `check-indirect-buffers`
/// the moment it did. That failed LOUDLY — the reconciliation reported the two
/// targets as advertised-but-undeclared — which is the whole reason this arm
/// compares two sets instead of trusting one. ADR 25.9.26l moved the private
/// half to `make/private/`, whose families sit two levels down
/// (`make/private/quality/*.mk`), so the walk no longer stops at one.
pub(crate) fn makefile_sources() -> Vec<PathBuf> {
    let repo_root = repo_root();
    let mut sources = vec![repo_root.join("Makefile")];
    let mut fragments = mk_files_in(&repo_root.join("make"));
    fragments.sort();
    sources.append(&mut fragments);
    sources
}

/// Does this checkout carry the private make half (ADR 25.9.26l D5)? The public
/// repository does not, so the targets it declares are not expected there.
pub(crate) fn private_make_present() -> bool {
    repo_root().join("make/private.mk").is_file()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the bootstrap crate has a parent directory")
        .to_path_buf()
}

/// Every `*.mk` in `dir` and, recursively, below it.
fn mk_files_in(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(Result::ok);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            found.extend(mk_files_in(&path));
        } else if path.extension().is_some_and(|ext| ext == "mk") {
            found.push(path);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MAKEFILE: &str = "\
# a comment: not a target
.PHONY: check-health lint

check-health: lint
\t$(CARGO) run -- check --json: not a target either
lint fmt:
\t$(CARGO) fmt
: leading colon
";

    fn targets_from_sample_makefile() -> HashSet<String> {
        make_targets_in(SAMPLE_MAKEFILE)
    }

    #[test]
    fn a_rule_header_declares_its_target() {
        assert!(targets_from_sample_makefile().contains("check-health"));
    }

    #[test]
    fn one_header_may_declare_several_targets() {
        let targets = targets_from_sample_makefile();
        assert!(targets.contains("lint"), "{targets:?}");
        assert!(targets.contains("fmt"), "{targets:?}");
    }

    #[test]
    fn a_comment_is_not_a_target() {
        let targets = targets_from_sample_makefile();
        assert!(!targets.contains("#"), "{targets:?}");
        assert!(!targets.contains("comment"), "{targets:?}");
    }

    #[test]
    fn an_indented_recipe_line_is_not_a_target() {
        // A `:` inside a recipe is shell syntax; treating it as a rule header
        // would inflate the declared set and make the reconciliation permissive.
        let targets = targets_from_sample_makefile();
        assert!(!targets.contains("$(CARGO)"), "{targets:?}");
        assert!(!targets.contains("run"), "{targets:?}");
    }

    #[test]
    fn a_directive_is_not_a_target() {
        assert!(
            !targets_from_sample_makefile().contains(".PHONY"),
            "{:?}",
            targets_from_sample_makefile()
        );
    }

    #[test]
    fn a_line_starting_with_a_colon_declares_nothing() {
        assert!(
            !targets_from_sample_makefile().contains("leading"),
            "{:?}",
            targets_from_sample_makefile()
        );
    }

    #[test]
    fn the_declared_set_is_exactly_the_rule_headers() {
        let mut targets: Vec<String> = targets_from_sample_makefile().into_iter().collect();
        targets.sort();
        assert_eq!(targets, vec!["check-health", "fmt", "lint"]);
    }

    /// The probe set spans the source LAYERS on purpose (ADR 31.7.26c): `lint`
    /// sits in `make/quality.mk`, `test-codegen` in another top-level fragment,
    /// and `check-ir-audits` in `make/quality/*.mk`. Before the descend was
    /// added, the nested ones went missing while the flat ones stayed green — so
    /// a probe set drawn only from the top level would have reported this
    /// extraction healthy.
    ///
    /// `check-health` and `mutants-diff` are private (`make/private/quality.mk`,
    /// `make/private/quality/mutation.mk`, two levels down), so they are probed
    /// only where that half is present; the public repository does not carry it.
    #[test]
    fn the_repos_own_makefiles_yield_known_targets() {
        let targets = declared_make_targets();
        let mut probes = vec!["lint", "test-codegen", "check-ir-audits"];
        if private_make_present() {
            probes.extend(["check-health", "mutants-diff"]);
        }
        for known in probes {
            assert!(
                targets.contains(known),
                "extraction has drifted from the .mk syntax: `{known}` not found"
            );
        }
    }

    /// The nested fragments are DISCOVERED, not merely reachable: a regression
    /// to a flat `read_dir` leaves this empty and the assertion says so.
    #[test]
    fn the_source_list_includes_the_make_subdirectory_fragments() {
        let nested: Vec<_> = makefile_sources()
            .into_iter()
            .filter(|p| p.parent().is_some_and(|d| d.ends_with("quality")))
            .collect();
        assert!(
            !nested.is_empty(),
            "make/quality/*.mk must be in the source list (ADR 31.7.26c)"
        );
    }

    /// Two levels down: `make/private/quality/*.mk` (ADR 25.9.26l). A walk that
    /// stops at one level leaves this empty wherever the private half exists.
    #[test]
    fn the_source_list_reaches_two_levels_below_make() {
        if !private_make_present() {
            return;
        }
        let deep: Vec<_> = makefile_sources()
            .into_iter()
            .filter(|p| p.to_string_lossy().contains("make/private/quality/"))
            .collect();
        assert!(
            !deep.is_empty(),
            "make/private/quality/*.mk must be in the source list"
        );
    }
}
