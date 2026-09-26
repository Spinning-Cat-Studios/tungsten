//! What the analysis concludes, and how a rejection renders
//! (ADR 19.8.26d).
//!
//! Split from the protocol tests next door because the two fail for different
//! reasons: a protocol defect loses a definition, a verdict defect keeps every
//! definition and answers wrongly about it. Both drive the **externs**, since
//! the marshalling is the half a pure-function test cannot reach.

use super::*;

#[test]
fn a_non_descending_self_call_is_rejected_through_the_seam() {
    tg_init();
    tg_termination_reset();
    unsafe {
        assert!(tg_termination_declare(cstr("loop")));
        assert!(tg_termination_add_def(
            cstr("loop"),
            nat_to_nat(),
            self_call("loop")
        ));
    }
    assert_eq!(
        tg_termination_plan(true),
        1,
        "the self-call is its own component"
    );
    unsafe {
        assert!(tg_termination_add_def(
            cstr("loop"),
            nat_to_nat(),
            self_call("loop")
        ));
    }
    assert_eq!(tg_termination_check(), 1);
    assert_eq!(tg_termination_failure_count(), 1);
    let (kind, function, message) = rendered(0);
    assert_eq!(kind, "0", "a descent failure, not the proof boundary");
    assert_eq!(function, "loop");
    assert!(!message.is_empty(), "the engine's own wording travels");
}
#[test]
fn a_partial_annotation_admits_the_same_definition() {
    tg_init();
    tg_termination_reset();
    unsafe {
        assert!(tg_termination_note_item(
            cstr("loop"),
            true,
            std::ptr::null(),
            false
        ));
        assert!(tg_termination_declare(cstr("loop")));
        assert!(tg_termination_add_def(
            cstr("loop"),
            nat_to_nat(),
            self_call("loop")
        ));
    }
    tg_termination_plan(true);
    unsafe {
        tg_termination_add_def(cstr("loop"), nat_to_nat(), self_call("loop"));
    }
    assert_eq!(
        tg_termination_check(),
        0,
        "`#[partial]` opts the group out of descent"
    );
}
#[test]
fn a_proof_reaching_a_partial_constant_is_the_other_error_kind() {
    tg_init();
    tg_termination_reset();
    unsafe {
        assert!(tg_termination_note_item(
            cstr("loop"),
            true,
            std::ptr::null(),
            false
        ));
        assert!(tg_termination_note_item(
            cstr("thm"),
            false,
            std::ptr::null(),
            true
        ));
        assert!(tg_termination_declare(cstr("loop")));
        assert!(tg_termination_declare(cstr("thm")));
        assert!(tg_termination_add_def(
            cstr("loop"),
            nat_to_nat(),
            self_call("loop")
        ));
        assert!(tg_termination_add_def(
            cstr("thm"),
            nat_to_nat(),
            self_call("loop")
        ));
    }
    tg_termination_plan(true);
    unsafe {
        tg_termination_add_def(cstr("loop"), nat_to_nat(), self_call("loop"));
        tg_termination_add_def(cstr("thm"), nat_to_nat(), self_call("loop"));
    }
    assert_eq!(tg_termination_check(), 1);
    let (kind, function, _) = rendered(0);
    assert_eq!(kind, "1", "the proof boundary is its own diagnostic");
    assert_eq!(function, "thm");
}
#[test]
fn a_taint_seed_survives_being_reduced_rather_than_retained() {
    // `helper` is `#[partial]` and NOT recursive, so the plan drops it and it
    // reaches the engine only through the carried channel. The proof must still
    // be rejected — this is the arm that fails open if the carried set loses
    // the annotation.
    tg_init();
    tg_termination_reset();
    unsafe {
        tg_termination_note_item(cstr("helper"), true, std::ptr::null(), false);
        tg_termination_note_item(cstr("thm"), false, std::ptr::null(), true);
        tg_termination_declare(cstr("helper"));
        tg_termination_declare(cstr("thm"));
        tg_termination_add_def(cstr("helper"), nat_to_nat(), tg_term_zero());
        tg_termination_add_def(
            cstr("thm"),
            nat_to_nat(),
            tg_term_app(tg_term_global(cstr("helper")), tg_term_zero()),
        );
    }
    assert_eq!(
        tg_termination_plan(true),
        0,
        "nothing here recurses, so nothing is retained"
    );
    unsafe {
        tg_termination_add_def(cstr("helper"), nat_to_nat(), tg_term_zero());
        tg_termination_add_def(
            cstr("thm"),
            nat_to_nat(),
            tg_term_app(tg_term_global(cstr("helper")), tg_term_zero()),
        );
    }
    assert_eq!(tg_termination_check(), 1);
    let (kind, function, _) = rendered(0);
    assert_eq!(kind, "1");
    assert_eq!(function, "thm");
}
#[test]
fn the_taint_half_alone_still_rejects_a_proof_and_never_a_recursion() {
    // What the self-hosted driver asks for: `plan(false)` retains nothing, so
    // descent has no terms to read and reports nothing — while the proof
    // boundary, which needs only the reduced name sets, still fires. This is
    // the configuration ADR 19.8.26d ships, and the assertion pair is the
    // point: silence from descent must NOT come with silence from taint.
    tg_init();
    tg_termination_reset();
    unsafe {
        tg_termination_note_item(cstr("looper"), true, std::ptr::null(), false);
        tg_termination_note_item(cstr("thm"), false, std::ptr::null(), true);
        tg_termination_declare(cstr("spins"));
        tg_termination_declare(cstr("looper"));
        tg_termination_declare(cstr("thm"));
        // `spins` recurses without descending — descent's business, and with
        // descent off it must be silent rather than rejected.
        tg_termination_add_def(cstr("spins"), nat_to_nat(), self_call("spins"));
        tg_termination_add_def(cstr("looper"), nat_to_nat(), self_call("looper"));
        tg_termination_add_def(
            cstr("thm"),
            nat_to_nat(),
            tg_term_app(tg_term_global(cstr("looper")), tg_term_zero()),
        );
    }
    assert_eq!(
        tg_termination_plan(false),
        0,
        "the taint half retains nothing"
    );
    assert_eq!(tg_termination_check(), 1);
    let (kind, function, _) = rendered(0);
    assert_eq!(kind, "1", "the proof boundary, not a descent failure");
    assert_eq!(function, "thm");
}
#[test]
fn descent_is_what_the_switch_turns_off() {
    // The must-fail twin of the test above, over the same `spins`: with
    // descent ON the same definition IS rejected. Without this pair, a
    // registry that had stopped analysing anything at all would pass the
    // taint-only test by accident.
    tg_init();
    tg_termination_reset();
    unsafe {
        tg_termination_declare(cstr("spins"));
        tg_termination_add_def(cstr("spins"), nat_to_nat(), self_call("spins"));
    }
    assert_eq!(tg_termination_plan(true), 1);
    unsafe {
        tg_termination_add_def(cstr("spins"), nat_to_nat(), self_call("spins"));
    }
    assert_eq!(tg_termination_check(), 1);
    assert_eq!(rendered(0).0, "0", "a descent failure this time");
}
#[test]
fn the_failure_count_tracks_the_rejections_rather_than_reporting_one() {
    // The count is a *bound* the caller loops to, so a reader that always said
    // "1" would render the first rejection and silently drop the rest — and
    // every single-rejection test would still pass. Three cardinalities, and
    // the zero is as load-bearing as the two.
    let count_for = |proofs: &[&str]| {
        tg_init();
        tg_termination_reset();
        unsafe {
            tg_termination_note_item(cstr("looper"), true, std::ptr::null(), false);
            tg_termination_declare(cstr("looper"));
            tg_termination_add_def(cstr("looper"), nat_to_nat(), self_call("looper"));
            for proof in proofs {
                tg_termination_note_item(cstr(proof), false, std::ptr::null(), true);
                tg_termination_declare(cstr(proof));
                tg_termination_add_def(
                    cstr(proof),
                    nat_to_nat(),
                    tg_term_app(tg_term_global(cstr("looper")), tg_term_zero()),
                );
            }
        }
        tg_termination_plan(false);
        tg_termination_check();
        tg_termination_failure_count()
    };

    assert_eq!(count_for(&[]), 0, "a partial constant alone is admissible");
    assert_eq!(count_for(&["thm"]), 1);
    assert_eq!(
        count_for(&["thm", "lemma"]),
        2,
        "both proofs are rejected, and both must be reachable"
    );
}
#[test]
fn every_rejection_renders_rather_than_only_the_first() {
    // The other half of the same hazard: a count of 2 is worth nothing if
    // index 1 renders null.
    tg_init();
    tg_termination_reset();
    unsafe {
        tg_termination_note_item(cstr("looper"), true, std::ptr::null(), false);
        tg_termination_declare(cstr("looper"));
        tg_termination_add_def(cstr("looper"), nat_to_nat(), self_call("looper"));
        for proof in ["alpha", "beta"] {
            tg_termination_note_item(cstr(proof), false, std::ptr::null(), true);
            tg_termination_declare(cstr(proof));
            tg_termination_add_def(
                cstr(proof),
                nat_to_nat(),
                tg_term_app(tg_term_global(cstr("looper")), tg_term_zero()),
            );
        }
    }
    tg_termination_plan(false);
    assert_eq!(tg_termination_check(), 2);
    let named: Vec<String> = (0..tg_termination_failure_count())
        .map(|index| rendered(index).1)
        .collect();
    assert_eq!(named, vec!["alpha".to_string(), "beta".to_string()]);
}
#[test]
fn a_retained_proof_keeps_its_role_across_the_second_stream() {
    // A proof that is ITSELF in a recursive group is retained rather than
    // carried, so its role travels through `add_def` rather than through the
    // carried set — a different path, and the one where losing `DefRole` would
    // turn an inadmissible proof into an ordinary partial function.
    tg_init();
    tg_termination_reset();
    unsafe {
        tg_termination_note_item(cstr("thm"), true, std::ptr::null(), true);
        tg_termination_declare(cstr("thm"));
        tg_termination_add_def(cstr("thm"), nat_to_nat(), self_call("thm"));
    }
    assert_eq!(
        tg_termination_plan(true),
        1,
        "it recurses, so it is retained"
    );
    unsafe {
        tg_termination_add_def(cstr("thm"), nat_to_nat(), self_call("thm"));
    }
    assert_eq!(tg_termination_check(), 1);
    let (kind, function, _) = rendered(0);
    assert_eq!(
        kind, "1",
        "a `#[partial]` proof is the proof boundary, not a descent failure"
    );
    assert_eq!(function, "thm");
}
