//! `doctor suggest-tools` through the real binary (ADR 4.9.26d).
//!
//! The unit tests next to the registry assert what the matcher *returns*; these
//! assert what a reader actually *gets*, which is a different thing and the one
//! that failed. A symptom query returning the right ranking in-process is no
//! use if the command prints nothing, and the empty answer — the surface D4 is
//! about — is only observable from outside the process.

use std::process::Command;

/// Run `tungsten doctor suggest-tools <args>` and return its stdout.
fn suggest(args: &[&str]) -> String {
    let finished = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .arg("doctor")
        .arg("suggest-tools")
        .args(args)
        .output()
        .expect("spawn tungsten doctor suggest-tools");
    assert!(
        finished.status.success(),
        "suggest-tools exited {:?} for {args:?}",
        finished.status.code()
    );
    String::from_utf8_lossy(&finished.stdout).into_owned()
}

/// AC 2, end to end: the query that returned "no matching diagnostic tools" on
/// 4 Sep 2026 now prints the self-host divergence commands to a real terminal.
#[test]
fn a_symptom_query_prints_the_divergence_tools() {
    let text = suggest(&["destructuring gives the wrong element"]);
    assert!(
        text.contains("diff bootstrap-selfhost-check"),
        "missing the divergence entry point: {text}"
    );
    assert!(
        text.contains("diff selfhost-core"),
        "missing selfhost-core: {text}"
    );
    assert!(
        text.contains("check nested-patterns"),
        "missing nested-patterns: {text}"
    );
}

/// The JSON form is what agent consumers read, and an empty stdout parses as
/// nothing at all — so assert a document comes back, with the fields the
/// consumer indexes.
#[test]
fn the_json_form_reaches_stdout_as_a_parseable_document() {
    let text = suggest(&["--json", "reading a field gives back the wrong value"]);
    let items: Vec<serde_json::Value> = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("stdout is not JSON: {e}\n{text}"));
    assert!(!items.is_empty(), "no suggestions in the JSON: {text}");
    assert!(items[0].get("command").is_some());
    assert!(items[0].get("cost").is_some());
    assert!(items[0].get("reason").is_some());
}

/// AC 6: a query that genuinely matches nothing still answers, and the answer
/// it gives now offers symptom phrasings rather than the cause-shaped keywords
/// that taught the reader they had to diagnose the problem first (D4).
#[test]
fn a_query_that_matches_nothing_prints_symptom_examples() {
    let text = suggest(&["xyzzy completely unrelated nonsense"]);
    assert!(
        text.contains("No matching diagnostic tools found"),
        "{text}"
    );
    assert!(text.contains("describe what you SAW"), "{text}");
    assert!(
        !text.contains("most relevant first"),
        "the empty answer must not read like a ranking: {text}"
    );
    // Every example the tip prints is itself a query that matches — asserted
    // exhaustively in the unit tests; spot-checked here as printed text.
    assert!(
        text.contains("'my recursive function is rejected'"),
        "{text}"
    );
}
