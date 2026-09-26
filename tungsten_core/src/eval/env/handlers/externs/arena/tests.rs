//! Tests for the type-arena arms (ADR 7.8.26c).
//!
//! The first test is the ADR's load-bearing one: it is what would fail if the
//! arena were reset between evaluated test bodies, and it was written before
//! any arm below it.

use crate::eval::{nat_to_term, term_to_nat, StepResult};
use crate::ffi::types::constructors;
use crate::terms::Term;

use super::step_arena_extern;

/// Step `name` with `values` and read the resulting handle back as a `usize`.
///
/// Panics rather than returning `Option` so a `Stuck` arm names itself in the
/// failure, instead of surfacing as a `None` three assertions later.
fn stepped_nat(name: &str, values: &[Term]) -> usize {
    let result = step_arena_extern(name, values)
        .unwrap_or_else(|| panic!("`{name}` is not claimed by the arena dispatcher"));
    match result {
        StepResult::Stepped(term) => {
            term_to_nat(&term).unwrap_or_else(|| panic!("`{name}` stepped to a non-Nat: {term:?}"))
        }
        other => panic!("`{name}` did not step on {values:?}: {other:?}"),
    }
}

/// Allocate a fresh `Nat` type node and return its handle.
fn fresh_nat_handle() -> usize {
    stepped_nat("tg_type_nat", &[])
}

// ===========================================================================
// D1 / AC3 — the arena is never reset, and that is what keeps handles sound
// ===========================================================================

/// A handle still reads back its own node after later allocations.
///
/// **This is the test that would fail under reset-per-test semantics** (ADR
/// 7.8.26c §2.1). Handles are plain `Vec` indices, so truncating `types`
/// between test bodies would leave this handle resolving to a *different*
/// node — a silently wrong tag, not `INVALID_HANDLE` and not a panic. The
/// arena is grow-only and thread-local, so the handle stays valid, and this
/// test is the executable half of that argument.
#[test]
fn an_earlier_handle_survives_later_allocations() {
    let nat = fresh_nat_handle();
    assert_eq!(
        stepped_nat("tg_type_tag", &[nat_to_term(nat)]),
        0,
        "tag 0 is Nat"
    );

    // Stand in for "another test body ran on this thread".
    for _ in 0..8 {
        let _ = stepped_nat("tg_type_bool", &[]);
        let _ = stepped_nat("tg_type_unit", &[]);
    }

    assert_eq!(
        stepped_nat("tg_type_tag", &[nat_to_term(nat)]),
        0,
        "the first handle must still read back as Nat; a reset would alias it \
         onto whichever node later took index {nat}"
    );
}

/// Distinct allocations get distinct handles — the premise the test above
/// rests on. If the arena handed out the same index twice, aliasing would be
/// invisible to that test rather than caught by it.
#[test]
fn each_allocation_gets_its_own_handle() {
    let first = fresh_nat_handle();
    let second = fresh_nat_handle();
    assert_ne!(first, second, "handles are Vec indices, never recycled");
}

// ===========================================================================
// Constructors
// ===========================================================================

/// Each nullary constructor allocates a node carrying its own tag.
#[test]
fn nullary_constructors_allocate_their_own_tag() {
    for (name, tag) in [("tg_type_nat", 0), ("tg_type_bool", 1), ("tg_type_unit", 3)] {
        let handle = stepped_nat(name, &[]);
        assert_eq!(
            stepped_nat("tg_type_tag", &[nat_to_term(handle)]),
            tag,
            "{name} should allocate a node with tag {tag}"
        );
    }
}

/// A sum is built from its two children and reads back with both, in order.
///
/// The order matters and is asserted rather than assumed: left/right swapped
/// would still tag as 8 and still round-trip, so only naming the children
/// distinguishes a correct arm from a transposed one.
#[test]
fn a_sum_reads_back_both_children_in_order() {
    let left = stepped_nat("tg_type_nat", &[]);
    let right = stepped_nat("tg_type_bool", &[]);
    let sum = stepped_nat("tg_type_sum", &[nat_to_term(left), nat_to_term(right)]);

    assert_eq!(
        stepped_nat("tg_type_tag", &[nat_to_term(sum)]),
        8,
        "tag 8 is Sum"
    );
    assert_eq!(
        stepped_nat("tg_type_get_sum_left", &[nat_to_term(sum)]),
        left
    );
    assert_eq!(
        stepped_nat("tg_type_get_sum_right", &[nat_to_term(sum)]),
        right
    );
}

/// An arrow, likewise — domain and codomain are not interchangeable.
#[test]
fn an_arrow_reads_back_domain_and_codomain_in_order() {
    let domain = stepped_nat("tg_type_nat", &[]);
    let codomain = stepped_nat("tg_type_bool", &[]);
    let arrow = stepped_nat(
        "tg_type_arrow",
        &[nat_to_term(domain), nat_to_term(codomain)],
    );

    assert_eq!(
        stepped_nat("tg_type_tag", &[nat_to_term(arrow)]),
        6,
        "tag 6 is Arrow"
    );
    assert_eq!(
        stepped_nat("tg_type_get_arrow_domain", &[nat_to_term(arrow)]),
        domain
    );
    assert_eq!(
        stepped_nat("tg_type_get_arrow_codomain", &[nat_to_term(arrow)]),
        codomain
    );
}

/// The μ case, end to end: the name goes out as a C string and comes back as
/// one, through the three arms §1.4 classes as "allocates a C string".
#[test]
fn a_mu_type_round_trips_its_binder_name() {
    let name_address = stepped_nat("tg_string_to_cstr", &[Term::StringLit("List".to_string())]);
    let body = stepped_nat("tg_type_nat", &[]);
    let mu = stepped_nat(
        "tg_type_mu",
        &[nat_to_term(name_address), nat_to_term(body)],
    );

    assert_eq!(
        stepped_nat("tg_type_tag", &[nat_to_term(mu)]),
        11,
        "tag 11 is Mu"
    );
    assert_eq!(stepped_nat("tg_type_get_mu_body", &[nat_to_term(mu)]), body);

    let var_address = stepped_nat("tg_type_get_mu_var", &[nat_to_term(mu)]);
    let read_back = step_arena_extern("tg_cstring_to_string", &[nat_to_term(var_address)]);
    assert_eq!(
        read_back,
        Some(StepResult::Stepped(Term::StringLit("List".to_string()))),
        "the binder name must survive the C-string round trip"
    );
}

// ===========================================================================
// The five accessors `type_handle_to_codegen_type` reaches but the six tests
// never trigger
// ===========================================================================
//
// `info eval reachable-externs` walks the *static* call graph, so these are on
// all six paths and must be claimed for `blocking` to reach 0 — even though no
// test constructs a Product, Forall or TyVar. Their constructors are therefore
// NOT registered (Non-Goals), and these tests build the nodes by calling
// `crate::ffi` directly. That keeps the registry at exactly what the tool
// demands while still driving each accessor arm through a real node.

/// A product reads back both children, in order.
#[test]
fn a_product_reads_back_both_children_in_order() {
    let left = stepped_nat("tg_type_nat", &[]);
    let right = stepped_nat("tg_type_bool", &[]);
    let product = constructors::tg_type_product(left as u64, right as u64);

    assert_eq!(
        stepped_nat("tg_type_tag", &[nat_to_term(product as usize)]),
        7,
        "tag 7 is Product"
    );
    assert_eq!(
        stepped_nat("tg_type_get_product_left", &[nat_to_term(product as usize)]),
        left
    );
    assert_eq!(
        stepped_nat(
            "tg_type_get_product_right",
            &[nat_to_term(product as usize)]
        ),
        right
    );
}

/// A forall reads back its binder and body, like μ but through its own pair.
#[test]
fn a_forall_reads_back_its_binder_and_body() {
    let body = stepped_nat("tg_type_bool", &[]);
    let forall = crate::ffi::test_support::forall_type_from_cstr(
        crate::ffi::cstr_address_of("T"),
        body as u64,
    );

    assert_eq!(
        stepped_nat("tg_type_tag", &[nat_to_term(forall as usize)]),
        10,
        "tag 10 is Forall"
    );
    assert_eq!(
        stepped_nat("tg_type_get_forall_body", &[nat_to_term(forall as usize)]),
        body
    );
    let var_address = stepped_nat("tg_type_get_forall_var", &[nat_to_term(forall as usize)]);
    assert_eq!(
        step_arena_extern("tg_cstring_to_string", &[nat_to_term(var_address)]),
        Some(StepResult::Stepped(Term::StringLit("T".to_string())))
    );
}

/// A type variable reads its name back, closing the third C-string accessor.
#[test]
fn a_tyvar_reads_its_name_back() {
    let tyvar = crate::ffi::test_support::tyvar_type_from_cstr(crate::ffi::cstr_address_of("α"));

    assert_eq!(
        stepped_nat("tg_type_tag", &[nat_to_term(tyvar as usize)]),
        9,
        "tag 9 is TyVar"
    );
    let read_back = stepped_nat("tg_type_get_tyvar_name", &[nat_to_term(tyvar as usize)]);
    assert_eq!(
        step_arena_extern("tg_cstring_to_string", &[nat_to_term(read_back)]),
        Some(StepResult::Stepped(Term::StringLit("α".to_string())))
    );
}

// ===========================================================================
// What the dispatcher does NOT claim
// ===========================================================================

/// An unrelated name falls through, so `call`'s own arms still get their turn.
#[test]
fn an_unrelated_extern_is_not_claimed() {
    assert!(step_arena_extern("tg_assert_eq_nat", &[nat_to_term(1), nat_to_term(1)]).is_none());
    assert!(step_arena_extern("tg_println", &[nat_to_term(0), nat_to_term(0)]).is_none());
}

/// `tg_init` is deliberately absent (ADR 7.8.26c Non-Goals): registering it
/// would let one evaluated test body invalidate every other body's handles.
#[test]
fn tg_init_is_not_claimed() {
    assert!(
        step_arena_extern("tg_init", &[]).is_none(),
        "tg_init resets the arena — §2.1 is the argument that no test body may"
    );
}

/// A wrong-arity call is not claimed, so it ends `Stuck` rather than being
/// coerced into a plausible-looking handle.
#[test]
fn a_wrong_arity_call_is_not_claimed() {
    assert!(step_arena_extern("tg_type_sum", &[nat_to_term(0)]).is_none());
    assert!(step_arena_extern("tg_type_tag", &[]).is_none());
    assert!(step_arena_extern("tg_string_to_cstr", &[nat_to_term(0)]).is_none());
}

/// A non-`Nat` handle operand stays `Stuck` rather than being guessed at —
/// the same refusal `console::free_cstring` makes for an address.
#[test]
fn a_non_nat_handle_operand_is_stuck() {
    assert_eq!(
        step_arena_extern("tg_type_tag", &[Term::Unit]),
        Some(StepResult::Stuck)
    );
    assert_eq!(
        step_arena_extern("tg_type_sum", &[Term::Unit, nat_to_term(0)]),
        Some(StepResult::Stuck)
    );
}

/// The same refusal on the three arms that carry an **address** rather than a
/// handle — the ones where guessing is worst.
///
/// A misread handle yields a wrong type and a confusing test failure. A misread
/// *address* is handed to `CStr::from_ptr` or to `tg_type_mu`, which reads
/// bytes at whatever number the bad cast produced. Each arm documents that it
/// refuses rather than coerces; these are the assertions making that true, and
/// they were the last three uncovered lines in this module.
#[test]
fn a_non_nat_address_operand_is_stuck_on_every_cstring_arm() {
    // tg_type_mu: the binder-name address, then the body handle.
    assert_eq!(
        step_arena_extern("tg_type_mu", &[Term::Unit, nat_to_term(0)]),
        Some(StepResult::Stuck),
        "a non-Nat binder address must not be dereferenced"
    );
    assert_eq!(
        step_arena_extern("tg_type_mu", &[nat_to_term(0), Term::Unit]),
        Some(StepResult::Stuck),
        "the body operand is checked too, not just the first"
    );

    // tg_type_get_mu_var and its two siblings read a name OUT of a node.
    for accessor in [
        "tg_type_get_mu_var",
        "tg_type_get_forall_var",
        "tg_type_get_tyvar_name",
    ] {
        assert_eq!(
            step_arena_extern(accessor, &[Term::Unit]),
            Some(StepResult::Stuck),
            "{accessor} must refuse a non-Nat handle"
        );
    }

    // tg_cstring_to_string reads bytes at the address it is given.
    assert_eq!(
        step_arena_extern("tg_cstring_to_string", &[Term::Unit]),
        Some(StepResult::Stuck),
        "a non-Nat address must not be read as a C string"
    );
}

/// An out-of-range handle yields `INVALID_HANDLE` from the real symbol rather
/// than panicking — the contract the accessors already document.
#[test]
fn an_out_of_range_handle_reads_back_as_invalid() {
    let nowhere = nat_to_term(usize::MAX - 1);
    assert_eq!(
        stepped_nat("tg_type_get_sum_left", std::slice::from_ref(&nowhere)),
        crate::ffi::INVALID_HANDLE as usize
    );
}
