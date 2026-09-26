//! The exit codes `doctor check module name-collisions` and `doctor check link` hand a
//! shell (ADR 13.8.26c).
//!
//! `ExitCode` implements neither `PartialEq` nor any accessor, so the number a
//! shell sees can only be asserted by spawning the binary — and here it is the
//! number that carries the whole of D3. The check is **advisory**: findings do
//! NOT gate, so `0` on a colliding tree is the decision, not an oversight, and
//! it has to be distinguishable from `2` on bad input. An in-process test can
//! reach neither.
//!
//! Fixtures are written into a tempdir rather than read from the repo, so the
//! assertions hold in a copied workspace (a mutation sweep) as well as in the
//! live checkout.
//!
//! The regrouped `doctor check type determinism` commands are exercised here
//! too. They dispatch through a library function returning `ExitCode`, which
//! implements neither `PartialEq` nor any accessor — spawning the binary is the
//! only way to assert the number a shell sees, and therefore the only way to
//! tell the dispatcher apart from one that always succeeds.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Two modules define a private `shared_helper`: the walk registers `beta`'s
/// last, so `alpha`'s call site reports E0016 and the tree does not elaborate.
const COLLIDING: &[(&str, &str)] = &[
    (
        "main.tg",
        "mod alpha;\n\
         mod beta;\n\
         use alpha::entry_alpha;\n\
         use beta::entry_beta;\n\
         fn main() -> Nat { entry_alpha() + entry_beta() }\n",
    ),
    (
        "alpha.tg",
        "fn shared_helper() -> Nat { 1 }\n\
         pub fn entry_alpha() -> Nat { shared_helper() }\n",
    ),
    (
        "beta.tg",
        "fn shared_helper() -> Nat { 2 }\n\
         pub fn entry_beta() -> Nat { shared_helper() }\n",
    ),
];

/// Write a module tree into a fresh tempdir and return it with the entry file.
fn fixture(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir");
    for (name, source) in files {
        std::fs::write(dir.path().join(name), source).expect("write fixture");
    }
    let entry = dir.path().join(files[0].0);
    (dir, entry)
}

/// Run `tungsten` with `args`, returning its exit code and combined output.
fn run(args: &[&str]) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(args)
        .output()
        .expect("spawn tungsten");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(-1), combined)
}

/// Run the check on `path` with any extra flags.
fn check(path: &Path, extra: &[&str]) -> (i32, String) {
    let mut args = vec!["doctor", "check", "module", "name-collisions"];
    args.push(path.to_str().expect("utf-8 path"));
    args.extend_from_slice(extra);
    run(&args)
}

/// D3, and the whole reason the ADR argues for it: findings do not gate.
/// **Under `--severity live` as much as under `all`** — the live classes are the
/// errors-today ones, and even those exit 0 on first landing.
#[test]
fn findings_do_not_gate_under_either_severity() {
    let (_dir, entry) = fixture(COLLIDING);

    for extra in [&[][..], &["--severity", "live"][..]] {
        let (code, output) = check(&entry, extra);
        assert_eq!(code, 0, "advisory on first landing ({extra:?}) — {output}");
        assert!(
            output.contains("shared_helper"),
            "and it did find the collision — {output}"
        );
    }
}

/// The input the whole design serves: a file the elaborator REJECTS. `check`
/// exits non-zero on it; the census exits 0 and still reports.
#[test]
fn the_check_reports_on_a_file_the_elaborator_rejects() {
    let (_dir, entry) = fixture(COLLIDING);

    let (elaborated, elaboration_output) = run(&["check", entry.to_str().expect("utf-8 path")]);
    assert_ne!(
        elaborated, 0,
        "the fixture must actually be rejected, or this proves nothing — {elaboration_output}"
    );
    assert!(
        elaboration_output.contains("E0016"),
        "and rejected for the reason this ADR is about — {elaboration_output}"
    );

    let (code, output) = check(&entry, &[]);
    assert_eq!(code, 0);
    assert!(output.contains("registered last, wins"), "{output}");
}

/// Bad input is **2**, distinct from the 0 a finding earns. It is the only
/// non-zero code this command returns, so if the two collapsed nothing would
/// notice.
#[test]
fn an_unreadable_entry_file_exits_two() {
    let (code, _) = check(Path::new("/nonexistent/definitely-not-here.tg"), &[]);
    assert_eq!(code, 2);
}

/// A clean tree exits 0 too — same code as a finding, by design, which is why
/// the reach line and not the exit status is what a reader interprets.
#[test]
fn a_clean_tree_exits_zero_with_its_reach_line() {
    let (_dir, entry) = fixture(&[("main.tg", "fn main() -> Nat { 0 }\n")]);

    let (code, output) = check(&entry, &[]);
    assert_eq!(code, 0);
    assert!(output.contains("✓ No name collisions"), "{output}");
    assert!(output.contains("examined 1 module(s)"), "{output}");
}

/// D5's sub-namespace forwards its callee's exit code rather than inventing
/// one, and the hidden alias reaches the same command.
#[test]
fn the_link_subnamespace_forwards_its_callees_exit_code() {
    let missing = "/nonexistent/definitely-not-a-binary";

    let (grouped, _) = run(&["doctor", "check", "link", "health", missing]);
    assert_ne!(grouped, 0, "a missing binary is not link-healthy");

    let (alias, _) = run(&["doctor", "check", "link-health", missing]);
    assert_eq!(
        alias, grouped,
        "the hidden flat alias reaches the same command"
    );

    let (_dir, real) = fixture(&[("main.tg", "fn main() -> Nat { 0 }\n")]);
    let (ok, _) = run(&[
        "doctor",
        "check",
        "module",
        "name-collisions",
        real.to_str().unwrap(),
    ]);
    assert_ne!(
        ok, grouped,
        "and success and failure are different numbers, so forwarding means something"
    );
}

/// `--json` through the real binary: the flag D3 points a gate at has to work
/// where a gate would call it, not only where a unit test does.
#[test]
fn the_json_flag_emits_parseable_json_from_the_binary() {
    let (_dir, entry) = fixture(COLLIDING);

    let (code, output) = check(&entry, &["--json"]);
    assert_eq!(code, 0, "advisory in JSON mode too — {output}");

    let parsed: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|e| panic!("not JSON: {e}\n{output}"));
    assert_eq!(parsed["collision_count"], 1);
    assert_eq!(parsed["collisions"][0]["name"], "shared_helper");
    assert_eq!(parsed["collisions"][0]["class"], "private-shadowed");
    assert_eq!(parsed["collisions"][0]["winner"]["kind"], "definition");
    assert_eq!(parsed["collisions"][0]["winner"]["module"], "beta");
    assert_eq!(
        parsed["modules_examined"], 3,
        "the reach line survives into the machine-readable form"
    );
}

/// A clean tree's JSON is still JSON, with the reach counts that tell it apart
/// from a run that examined nothing.
#[test]
fn a_clean_trees_json_carries_the_reach_counts() {
    let (_dir, entry) = fixture(&[("main.tg", "fn main() -> Nat { 0 }\n")]);

    let (code, output) = check(&entry, &["--json"]);
    assert_eq!(code, 0);

    let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(parsed["collision_count"], 0);
    assert_eq!(parsed["modules_examined"], 1);
    assert_eq!(
        parsed["names_considered"], 1,
        "0 collisions and 0 names examined stay distinguishable in JSON"
    );
}

/// The `determinism` sub-namespace forwards its callees' exit codes rather than
/// inventing one, under both the grouped spelling and its hidden flat alias.
///
/// Bad input, not a finding: all three checks want a file they can elaborate,
/// so a path that does not exist must fail. A dispatcher that returned success
/// unconditionally would pass every other test in the suite.
#[test]
fn the_determinism_subnamespace_forwards_its_callees_exit_code() {
    let missing = "/nonexistent/definitely-not-here.tg";

    for (grouped, alias) in [
        (
            &["type", "determinism", "normalization"][..],
            &["type", "normalization-consistency"][..],
        ),
        (
            &["type", "determinism", "encoding"][..],
            &["type", "encoding-determinism"][..],
        ),
        (
            &["type", "determinism", "resolution-attempts"][..],
            &["type", "resolution-attempt-determinism"][..],
        ),
    ] {
        let mut g = grouped.to_vec();
        g.insert(0, "check");
        g.insert(0, "doctor");
        g.push(missing);
        let mut a = alias.to_vec();
        a.insert(0, "check");
        a.insert(0, "doctor");
        a.push(missing);

        let (grouped_code, _) = run(&g);
        let (alias_code, _) = run(&a);

        assert_ne!(
            grouped_code, 0,
            "{grouped:?} must fail on a file it cannot read"
        );
        assert_eq!(
            grouped_code, alias_code,
            "{grouped:?} and its alias {alias:?} must agree"
        );
    }

    // And a tree it CAN read succeeds — without this, "always fails" would pass
    // the assertions above just as well as the real dispatcher.
    let (_dir, entry) = fixture(&[("main.tg", "fn main() -> Nat { 0 }\n")]);
    let (ok, output) = run(&[
        "doctor",
        "check",
        "type",
        "determinism",
        "encoding",
        entry.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(ok, 0, "a readable tree checks clean — {output}");
}
