//! Parsing the self-host's term line, and what a comparison concludes.

use super::render::{
    compare, exit_code_for, first_difference, parse_core_line, preflight, tail_from,
    CoreComparison, MissingInput, SelfhostTerm,
};

/// What `--dump-core-terms probe_len` prints on a developer build.
const DUMPED: &str = "\
✓ build/probe.tg: 2 definition(s), all OK
[core] probe_len\tλl:μα_L2. (Unit + (Nat × α_L2)). zero
[dump-core] 1 of 2 definition(s) matched `probe_len`";

#[test]
fn the_term_is_read_back_verbatim() {
    assert_eq!(
        parse_core_line(DUMPED, "probe_len"),
        SelfhostTerm::Rendered("λl:μα_L2. (Unit + (Nat × α_L2)). zero".to_string())
    );
}

#[test]
fn the_wanted_definition_is_matched_by_name_not_by_position() {
    // `--dump-core-terms '*'` prints every definition; taking the first line
    // would compare against whichever one the walk emitted first.
    let many = "\
[core] alpha\tzero
[core] beta\tsucc zero
[dump-core] 2 of 2 definition(s) matched `*`";
    assert_eq!(
        parse_core_line(many, "beta"),
        SelfhostTerm::Rendered("succ zero".to_string())
    );
}

#[test]
fn a_name_that_prefixes_another_is_not_matched() {
    // `len` must not match `len2`'s line — the tab is part of the key for
    // exactly this reason. The census line is present so this isolates the
    // prefix question from "did the dump run at all".
    let many = "\
[core] len2\tsucc zero
[dump-core] 1 of 2 definition(s) matched `len2`";
    assert_eq!(parse_core_line(many, "len"), SelfhostTerm::NotFound);
}

#[test]
fn a_stubbed_build_is_distinguished_from_a_missing_definition() {
    let stubbed = "[diagnostics] this binary has no diagnostic tools compiled in";
    assert_eq!(parse_core_line(stubbed, "probe_len"), SelfhostTerm::Stubbed);
}

#[test]
fn a_definition_the_selfhost_does_not_have_is_not_found() {
    let none = "[dump-core] 0 of 2 definition(s) matched `nosuch`";
    assert_eq!(parse_core_line(none, "nosuch"), SelfhostTerm::NotFound);
}

#[test]
fn a_binary_that_never_dumped_is_unsupported_not_a_missing_definition() {
    // An older self-host skips an unknown flag silently and then reads its
    // argument as the filename, so it prints no dump census at all. Reporting
    // that as "no such definition" sends the reader hunting for a name that is
    // right there; the fix is to rebuild the binary.
    let older = "✓ build/probe.tg: 2 definition(s), all OK";
    assert_eq!(
        parse_core_line(older, "probe_len"),
        SelfhostTerm::Unsupported
    );
}

#[test]
fn an_unreadable_marker_is_not_compared_as_if_it_were_a_term() {
    // The self-host writes `<unreadable>` rather than an empty field, and this
    // must not be diffed against a real term — that would report a divergence
    // where the truth is a missing answer.
    let bad = "[core] probe_len\t<unreadable>";
    assert_eq!(parse_core_line(bad, "probe_len"), SelfhostTerm::Unreadable);
}

#[test]
fn identical_renderings_agree() {
    let term = "λl:Nat. zero";
    assert_eq!(
        compare(term, &SelfhostTerm::Rendered(term.to_string())),
        CoreComparison::Agree(term.to_string())
    );
}

#[test]
fn a_divergence_reports_where_it_starts() {
    // The real shape: identical prefix, then one side binds `t` and the other
    // leaves it free.
    let ours = "case x of inl a => zero | inr r => let t = snd r in t";
    let theirs = "case x of inl a => zero | inr r => t";
    let CoreComparison::Diverge {
        first_difference, ..
    } = compare(ours, &SelfhostTerm::Rendered(theirs.to_string()))
    else {
        panic!("expected a divergence");
    };
    assert_eq!(
        first_difference,
        "case x of inl a => zero | inr r => ".len()
    );
}

#[test]
fn every_unanswerable_reason_survives_into_the_comparison() {
    // The caller prints a different remedy per reason, so the comparison must
    // carry which one it was rather than collapsing them to "failed".
    for reason in [
        SelfhostTerm::Stubbed,
        SelfhostTerm::NotFound,
        SelfhostTerm::Unsupported,
        SelfhostTerm::NotExecutable,
        SelfhostTerm::Unreadable,
    ] {
        assert_eq!(
            compare("anything", &reason),
            CoreComparison::Unanswerable(reason.clone()),
            "{reason:?} was not carried through"
        );
    }
}

#[test]
fn a_binary_built_for_another_platform_is_its_own_verdict() {
    // Hit on the first end-to-end run: a Linux `tungsten1` invoked from the
    // macOS host. "Rebuild it" and "run it in the container" are different
    // fixes, so this must not fall through to `Unsupported`.
    let wrong_arch = "./tungsten1: ./tungsten1: cannot execute binary file";
    assert_eq!(
        parse_core_line(wrong_arch, "probe_len"),
        SelfhostTerm::NotExecutable
    );
}

#[test]
fn agreement_exits_zero_divergence_one_and_unanswerable_two() {
    assert_eq!(exit_code_for(&CoreComparison::Agree("t".into())), 0);
    assert_eq!(
        exit_code_for(&CoreComparison::Diverge {
            bootstrap: "a".into(),
            selfhost: "b".into(),
            first_difference: 0,
        }),
        1
    );
    assert_eq!(
        exit_code_for(&CoreComparison::Unanswerable(SelfhostTerm::Stubbed)),
        2,
        "could-not-ask is a different failure from found-a-difference"
    );
}

#[test]
fn the_first_difference_of_a_shared_prefix_is_where_it_ends() {
    assert_eq!(first_difference("abcdef", "abcXef"), 3);
}

#[test]
fn one_string_being_a_prefix_of_the_other_differs_at_the_shorter_length() {
    // No byte disagrees, so `position` finds nothing — the answer must be the
    // point where one ran out, not 0 and not a panic.
    assert_eq!(first_difference("abc", "abcdef"), 3);
    assert_eq!(first_difference("abcdef", "abc"), 3);
}

#[test]
fn the_tail_starts_at_the_difference_and_is_bounded() {
    assert_eq!(tail_from("abcdef", 3), "def");
    assert_eq!(tail_from("abcdef", 0), "abcdef");
    assert_eq!(
        tail_from(&"x".repeat(200), 0).len(),
        48,
        "a long term must not print in full at the difference marker"
    );
}

#[test]
fn a_tail_index_past_the_end_is_empty_rather_than_a_panic() {
    // `first_difference` can return the shorter length, which for the shorter
    // string is one past its last byte.
    assert_eq!(tail_from("abc", 3), "");
    assert_eq!(tail_from("abc", 99), "");
}

#[test]
fn the_tail_lands_on_a_character_boundary_in_a_term_full_of_greek() {
    // Real terms are full of μ, α and ×; slicing at a raw byte index inside one
    // would panic, so the tail must advance to the next boundary.
    let term = "μα_L2. (Unit + (Nat × α_L2))";
    for index in 0..term.len() {
        let _ = tail_from(term, index);
    }
}

#[test]
fn preflight_refuses_an_absent_selfhost_binary() {
    let existing = std::env::current_exe().expect("the test binary has a path");
    assert_eq!(preflight(&existing), Ok(()));
    assert_eq!(
        preflight(std::path::Path::new("/nonexistent/tungsten1")),
        Err(MissingInput::SelfhostBinary)
    );
}
