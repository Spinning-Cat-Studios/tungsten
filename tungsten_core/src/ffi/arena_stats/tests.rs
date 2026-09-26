//! Tests for the arena retention walkers and marker line (ADR 2.7.26a §3.4).

use super::*;

#[test]
fn scalar_types_own_no_heap() {
    assert_eq!(deep_type_bytes(&Type::Bool), 0);
    assert_eq!(deep_type_bytes(&Type::Error), 0);
}

#[test]
fn tyvar_counts_string_capacity() {
    let name = String::from("alpha");
    let expected = name.capacity() as u64;
    assert_eq!(deep_type_bytes(&Type::TyVar(name)), expected);
}

#[test]
fn arrow_counts_both_boxed_children() {
    let ty = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool));
    // Two boxed leaf children: 2 * size_of::<Type>(), no further heap.
    assert_eq!(deep_type_bytes(&ty), 2 * size_of::<Type>() as u64);
}

#[test]
fn nested_arrow_accumulates_recursively() {
    let inner = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool));
    let inner_bytes = deep_type_bytes(&inner);
    let ty = Type::Arrow(Box::new(inner), Box::new(Type::Unit));
    assert_eq!(
        deep_type_bytes(&ty),
        2 * size_of::<Type>() as u64 + inner_bytes
    );
}

#[test]
fn mu_counts_binder_name_and_body() {
    let var = String::from("α_List");
    let var_cap = var.capacity() as u64;
    let ty = Type::Mu(var, Box::new(Type::Nat));
    assert_eq!(deep_type_bytes(&ty), var_cap + size_of::<Type>() as u64);
}

#[test]
fn term_var_counts_name_capacity() {
    let name = String::from("x");
    let expected = name.capacity() as u64;
    assert_eq!(deep_term_bytes(&Term::Var(name)), expected);
}

#[test]
fn term_app_counts_boxed_children() {
    let t = Term::App(Box::new(Term::Zero), Box::new(Term::True));
    assert_eq!(deep_term_bytes(&t), 2 * size_of::<Term>() as u64);
}

#[test]
fn lambda_counts_var_type_and_body() {
    let var = String::from("x");
    let var_cap = var.capacity() as u64;
    let t = Term::Lambda(var, Type::Nat, Box::new(Term::Zero));
    assert_eq!(deep_term_bytes(&t), var_cap + size_of::<Term>() as u64);
}

#[test]
fn ctx_counts_bindings() {
    let ctx = Context::new().with_term("x", Type::Nat);
    let bytes = deep_ctx_bytes(&ctx);
    // At least the bindings vec slab + the name's capacity.
    assert!(bytes >= size_of::<Binding>() as u64 + 1);
}

#[test]
fn marker_line_reports_counts() {
    let mut arena = Arena::new();
    arena.alloc_type_node(crate::ffi::types::nodes::TypeNode::Nat);
    let line = marker_line(&arena);
    assert!(line.contains("types=1"), "line: {line}");
    assert!(line.contains("[arena]"), "line: {line}");
}

/// Cross-check `Term::for_each_embedded_type` against `deep_term_bytes` for
/// every variant: the two must agree on WHICH variants carry a `Type`.
///
/// The sample list is built twice — once with a heap-free `Type::Nat` in every
/// type slot, once with a heap-owning `Type::TyVar`. `deep_term_bytes`
/// differing between the two is *independent* evidence that the variant has a
/// type slot; the traversal must then yield it, and must yield nothing for the
/// variants where the size walker sees no difference.
///
/// This one is a soundness guard, not a metrics guard. `types::positivity`
/// reads the types embedded in `Eq` witness terms through
/// `for_each_embedded_type` (ADR 7.8.26e §2.1), so a `Type` added to a variant
/// and forgotten there is a missed *occurrence* — a false accept in the
/// strict-positivity gate. The three walkers' doc comments say "update all
/// three"; this is what makes that instruction enforceable.
#[test]
fn embedded_type_walker_agrees_with_the_size_walker_on_every_variant() {
    let heap_free = sample_of_every_term_variant_with(&Type::Nat);
    let heap_owning = sample_of_every_term_variant_with(&Type::TyVar("marker".to_string()));
    assert_eq!(heap_free.len(), heap_owning.len());

    let mut type_carrying = 0;
    for (plain, marked) in heap_free.iter().zip(&heap_owning) {
        let size_walker_sees_a_type = deep_term_bytes(marked) > deep_term_bytes(plain);
        let mut yielded = 0;
        marked.for_each_embedded_type(|_| yielded += 1);
        assert_eq!(
            size_walker_sees_a_type,
            yielded > 0,
            "walkers disagree on whether this variant carries a Type: {marked:?}"
        );
        if size_walker_sees_a_type {
            type_carrying += 1;
        }
    }
    // Non-vacuity: agreement on "no variant carries a type" would also pass.
    assert_eq!(
        type_carrying, 15,
        "expected 15 type-carrying Term variants; update when the enum grows"
    );
}

/// One minimal sample of EVERY `Term` variant: zero-capacity strings,
/// leaf `Type::Nat` annotations, `Term::Zero` children — so the only
/// heap a sample owns is its boxed/vec'd children.
fn minimal_sample_of_every_term_variant() -> Vec<Term> {
    sample_of_every_term_variant_with(&Type::Nat)
}

/// The same sample list with `annotation` in every embedded-`Type` slot.
///
/// Parameterized so a caller can make the type slots the *only* heap a sample
/// owns, which is what turns `deep_term_bytes` into an independent oracle for
/// "does this variant carry a `Type`?".
fn sample_of_every_term_variant_with(annotation: &Type) -> Vec<Term> {
    let s = String::new;
    let z = || Box::new(Term::Zero);
    let t = || annotation.clone();
    vec![
        Term::Var(s()),
        Term::Global(s()),
        Term::Lambda(s(), t(), z()),
        Term::App(z(), z()),
        Term::Let(s(), t(), z(), z()),
        Term::True,
        Term::False,
        Term::If(z(), z(), z()),
        Term::Unit,
        Term::Absurd(t(), z()),
        Term::Zero,
        Term::Succ(z()),
        Term::NatLit(7),
        Term::NatRec(t(), z(), z(), z()),
        Term::NatInd(t(), z(), z(), z()),
        Term::NatAdd(z(), z()),
        Term::NatSub(z(), z()),
        Term::NatMul(z(), z()),
        Term::NatDiv(z(), z()),
        Term::NatMod(z(), z()),
        Term::NatEq(z(), z()),
        Term::NatLt(z(), z()),
        Term::NatLe(z(), z()),
        Term::NatGt(z(), z()),
        Term::NatGe(z(), z()),
        Term::BoolAnd(z(), z()),
        Term::BoolOr(z(), z()),
        Term::BoolNot(z()),
        Term::StringLit(s()),
        Term::StrConcat(z(), z()),
        Term::StrLen(z()),
        Term::StrEq(z(), z()),
        Term::StrCharAt(z(), z()),
        Term::StrSubstring(z(), z(), z()),
        Term::Pair(z(), z()),
        Term::Fst(z()),
        Term::Snd(z()),
        Term::Inl(t(), z()),
        Term::Inr(t(), z()),
        Term::Case(z(), s(), z(), s(), z()),
        Term::TyAbs(s(), z()),
        Term::TyApp(z(), t()),
        Term::Refl(t(), z()),
        Term::Subst(t(), t(), z(), z()),
        Term::Fix(s(), t(), z()),
        Term::Fold(t(), z()),
        Term::Unfold(t(), z()),
        Term::ExternCall(s(), vec![Term::Zero]),
        Term::RefNew(z()),
        Term::RefGet(z()),
        Term::RefSet(z(), z()),
        Term::Annot(z(), t()),
        Term::Sorry,
        Term::AdtConstruct(t(), 0, z()),
        Term::AdtMatch(z(), vec![(0, s(), z())]),
        Term::Return(z()),
        Term::Spanned(z(), crate::terms::TermSpan::default()),
    ]
}

/// Pin `vm_rss_kb`'s platform contract (kills the `-> Some(0)` mutant):
/// no `/proc` off-Linux; a live test process is well over 0 KiB on Linux.
#[test]
fn vm_rss_reads_proc_or_nothing() {
    #[cfg(not(target_os = "linux"))]
    assert_eq!(vm_rss_kb(), None);
    #[cfg(target_os = "linux")]
    assert!(vm_rss_kb().unwrap() > 1024);
}

/// Pin the `/proc/self/status` parsing exactly (platform-independent).
#[test]
fn vm_rss_parsing_is_exact() {
    let status = "Name:\ttest\nVmPeak:\t  999 kB\nVmRSS:\t  31283 kB\nThreads:\t8\n";
    assert_eq!(parse_vm_rss_kb(status), Some(31283));
    // Must match VmRSS specifically, not VmPeak or a substring.
    assert_eq!(parse_vm_rss_kb("VmPeak:\t 999 kB\n"), None);
    assert_eq!(parse_vm_rss_kb("VmRSS:\n"), None);
    assert_eq!(parse_vm_rss_kb("VmRSS:\tnot-a-number kB\n"), None);
    assert_eq!(parse_vm_rss_kb(""), None);
}

/// Cross-check `deep_term_bytes` against `Term::for_each_subterm` for
/// every variant: with minimal samples, the size walker must account at
/// least `size_of::<Term>() + deep` for each child the traversal visits.
/// A boxed child missed in `deep_term_bytes` makes the total fall short
/// (the samples own no other heap large enough to mask it), so this
/// fails when the two match lists drift apart.
#[test]
fn walkers_agree_on_children_for_every_variant() {
    let samples = minimal_sample_of_every_term_variant();
    // Non-vacuity + completeness: one sample per `Term` variant. Update this
    // count when the enum grows — an emptied or stale sample list would make
    // this test pass while checking nothing.
    assert_eq!(
        samples.len(),
        57,
        "sample list must cover every Term variant"
    );
    for term in samples {
        let mut children_bytes = 0u64;
        term.for_each_subterm(|child| {
            children_bytes += size_of::<Term>() as u64 + deep_term_bytes(child);
        });
        let deep = deep_term_bytes(&term);
        assert!(
            deep >= children_bytes,
            "deep_term_bytes misses a child of {term:?}: deep={deep} < children={children_bytes}"
        );
    }
}
