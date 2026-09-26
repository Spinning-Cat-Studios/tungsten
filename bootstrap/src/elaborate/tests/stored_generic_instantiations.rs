//! Stored-position generic instantiation matrix (ADR 21.7.26e wall 1).
//!
//! A generic ADT instantiation in a *stored position* — a record field or a
//! constructor payload — must normalize equal to a fresh spelling of the same
//! type at a use site. ADR 21.7.26c hit `E0010: expected 'StrMap<CtorBucket>',
//! found 'StrMap<CtorBucket>'` (identically rendered) on every such shape for
//! a newly declared k=2 generic recursive ADT, while `List<…>` fields worked.
//!
//! The matrix axes (ADR 21.7.26e §2.1):
//!   position ∈ { record field, ctor payload (2-ctor sum) }
//!   generic  ∈ { List (control, k=1), fresh Tree<V> (k=2, the StrMap shape) }
//!   argument ∈ { builtin Nat, record, 2-ctor sum, nested List<Record> }
//!
//! The cross-module axis lives in tests/golden/check/cross_module_stored_*.
//!
//! Each cell exercises three flows over the stored instantiation:
//!   1. projection: stored position → fresh-spelled return type
//!   2. construction: fresh-spelled parameter → stored position
//!   3. generic round-trip: stored value through a generic `G<X> -> G<X>`
//!      function and back into the stored position (the `strmap_insert` shape
//!      that failed in the 21.7.26c session).

use crate::elaborate::tests::elab_ok;

/// Position of the stored generic instantiation under test.
#[derive(Clone, Copy)]
enum StoredPosition {
    RecordField,
    CtorPayload,
}

/// The generic ADT hosting the instantiation.
#[derive(Clone, Copy)]
enum GenericAdt {
    /// `List<T>` — k=1, the one stored shape that has always worked (control).
    ListControl,
    /// Fresh `Tree<V>` with two recursive occurrences per node — the exact
    /// `StrMap` shape from the bug report (k=2 α-occurrences).
    TreeK2,
}

/// The instantiation argument category.
#[derive(Clone, Copy)]
enum ArgKind {
    BuiltinNat,
    Record,
    TwoCtorSum,
    NestedListOfRecord,
}

impl GenericAdt {
    fn name(self) -> &'static str {
        match self {
            GenericAdt::ListControl => "List",
            GenericAdt::TreeK2 => "Tree",
        }
    }
}

impl ArgKind {
    fn type_spelling(self) -> &'static str {
        match self {
            ArgKind::BuiltinNat => "Nat",
            ArgKind::Record => "Bucket",
            ArgKind::TwoCtorSum => "Flag",
            ArgKind::NestedListOfRecord => "List<Bucket>",
        }
    }
}

/// Type definitions needed by a matrix cell: the argument type(s), then the
/// generic ADT hosting the stored instantiation.
fn cell_type_defs(generic: GenericAdt, arg: ArgKind) -> String {
    let mut defs = String::new();
    match arg {
        ArgKind::BuiltinNat => {}
        ArgKind::Record | ArgKind::NestedListOfRecord => {
            defs.push_str("type Bucket = { count: Nat, label: String }\n");
        }
        ArgKind::TwoCtorSum => {
            defs.push_str("type Flag = FlagOff | FlagOn(Nat)\n");
        }
    }
    let needs_list =
        matches!(generic, GenericAdt::ListControl) || matches!(arg, ArgKind::NestedListOfRecord);
    if needs_list {
        defs.push_str("type List<T> = Nil | Cons(T, List<T>)\n");
    }
    if matches!(generic, GenericAdt::TreeK2) {
        defs.push_str("type Tree<V> = TreeLeaf | TreeNode(Tree<V>, String, V, Tree<V>, Nat)\n");
    }
    defs
}

/// Build one matrix cell's source. Every cell defines four functions, so a
/// green cell asserts `defs.len() == 4`.
fn matrix_cell_source(position: StoredPosition, generic: GenericAdt, arg: ArgKind) -> String {
    let g = generic.name();
    let inst = format!("{}<{}>", g, arg.type_spelling());
    let mut src = cell_type_defs(generic, arg);
    // The generic pass-through used by flow 3 (fresh generic signature).
    src.push_str(&format!(
        "fn touch_generic<X>(g: {g}<X>) -> {g}<X> {{ g }}\n"
    ));
    match position {
        StoredPosition::RecordField => src.push_str(&format!(
            r#"
type Store = {{ stored: {inst} }}

fn project_stored(s: Store) -> {inst} {{ s.stored }}

fn rebuild_store(fresh: {inst}) -> Store {{
    {{ stored: fresh }}
}}

fn pass_stored_through_generic(s: Store) -> Store {{
    {{ stored: touch_generic(s.stored) }}
}}
"#
        )),
        StoredPosition::CtorPayload => src.push_str(&format!(
            r#"
type Holder = HolderEmpty | HolderFull({inst})

fn project_held(h: Holder, fallback: {inst}) -> {inst} {{
    match h {{
        HolderFull(g) => g,
        HolderEmpty() => fallback
    }}
}}

fn hold(fresh: {inst}) -> Holder {{ HolderFull(fresh) }}

fn pass_held_through_generic(h: Holder, fallback: {inst}) -> Holder {{
    HolderFull(touch_generic(project_held(h, fallback)))
}}
"#
        )),
    }
    src
}

fn assert_cell_elaborates(position: StoredPosition, generic: GenericAdt, arg: ArgKind) {
    let source = matrix_cell_source(position, generic, arg);
    let defs = elab_ok(&source);
    assert_eq!(defs.len(), 4, "cell source:\n{}", source);
}

// ─────────────────────────────────────────────────────────────────────────────
// Record field × List control (expected green throughout: the working shape)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn stored_field_list_of_nat() {
    assert_cell_elaborates(
        StoredPosition::RecordField,
        GenericAdt::ListControl,
        ArgKind::BuiltinNat,
    );
}

#[test]
fn stored_field_list_of_record() {
    assert_cell_elaborates(
        StoredPosition::RecordField,
        GenericAdt::ListControl,
        ArgKind::Record,
    );
}

#[test]
fn stored_field_list_of_sum() {
    assert_cell_elaborates(
        StoredPosition::RecordField,
        GenericAdt::ListControl,
        ArgKind::TwoCtorSum,
    );
}

#[test]
fn stored_field_list_of_nested_list() {
    assert_cell_elaborates(
        StoredPosition::RecordField,
        GenericAdt::ListControl,
        ArgKind::NestedListOfRecord,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Record field × fresh Tree k=2 (the bug-report shape)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn stored_field_tree_of_nat() {
    assert_cell_elaborates(
        StoredPosition::RecordField,
        GenericAdt::TreeK2,
        ArgKind::BuiltinNat,
    );
}

#[test]
fn stored_field_tree_of_record() {
    // The exact `dedup_index: StrMap<CtorBucket>` shape from the bug report.
    assert_cell_elaborates(
        StoredPosition::RecordField,
        GenericAdt::TreeK2,
        ArgKind::Record,
    );
}

#[test]
fn stored_field_tree_of_sum() {
    assert_cell_elaborates(
        StoredPosition::RecordField,
        GenericAdt::TreeK2,
        ArgKind::TwoCtorSum,
    );
}

#[test]
fn stored_field_tree_of_nested_list() {
    // `StrMap<List<ConstructorInfo>>` — the first shape tried in 21.7.26c.
    assert_cell_elaborates(
        StoredPosition::RecordField,
        GenericAdt::TreeK2,
        ArgKind::NestedListOfRecord,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Ctor payload × List control
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn stored_payload_list_of_nat() {
    assert_cell_elaborates(
        StoredPosition::CtorPayload,
        GenericAdt::ListControl,
        ArgKind::BuiltinNat,
    );
}

#[test]
fn stored_payload_list_of_record() {
    assert_cell_elaborates(
        StoredPosition::CtorPayload,
        GenericAdt::ListControl,
        ArgKind::Record,
    );
}

#[test]
fn stored_payload_list_of_sum() {
    assert_cell_elaborates(
        StoredPosition::CtorPayload,
        GenericAdt::ListControl,
        ArgKind::TwoCtorSum,
    );
}

#[test]
fn stored_payload_list_of_nested_list() {
    assert_cell_elaborates(
        StoredPosition::CtorPayload,
        GenericAdt::ListControl,
        ArgKind::NestedListOfRecord,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3-ctor generic (ADR 2.2.26 `Type::Adt` sum policy — the canonical encoder
// uses Adt for 3+ constructors while the old normalize-side copy built
// right-nested Sum chains, so a 3-ctor generic in a stored position is a
// distinct divergence surface from the 2-ctor cells above)
// ─────────────────────────────────────────────────────────────────────────────

/// A 3-ctor generic recursive ADT with multi-field constructors (k=2).
const POLY3_DEFS: &str = r#"
type Bucket = { count: Nat, label: String }
type Poly3<V> = P3Zero | P3One(V, String, Poly3<V>) | P3Two(Poly3<V>, V, Poly3<V>)
fn touch_poly3<X>(g: Poly3<X>) -> Poly3<X> { g }
"#;

#[test]
fn stored_field_three_ctor_generic_of_record() {
    let source = format!(
        r#"{POLY3_DEFS}
type Store = {{ stored: Poly3<Bucket> }}

fn project_stored(s: Store) -> Poly3<Bucket> {{ s.stored }}

fn pass_stored_through_generic(s: Store) -> Store {{
    {{ stored: touch_poly3(s.stored) }}
}}
"#
    );
    let defs = elab_ok(&source);
    assert_eq!(defs.len(), 3, "cell source:\n{}", source);
}

#[test]
fn stored_payload_three_ctor_generic_of_record() {
    let source = format!(
        r#"{POLY3_DEFS}
type Holder = HolderEmpty | HolderFull(Poly3<Bucket>)

fn project_held(h: Holder, fallback: Poly3<Bucket>) -> Poly3<Bucket> {{
    match h {{
        HolderFull(g) => g,
        HolderEmpty() => fallback
    }}
}}

fn hold_through_generic(h: Holder, fallback: Poly3<Bucket>) -> Holder {{
    HolderFull(touch_poly3(project_held(h, fallback)))
}}
"#
    );
    let defs = elab_ok(&source);
    assert_eq!(defs.len(), 3, "cell source:\n{}", source);
}

// ─────────────────────────────────────────────────────────────────────────────
// Expansion-depth backstop (normalize_app's MAX_APP_EXPANSION_DEPTH)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn deeply_nested_instantiation_hits_depth_backstop_without_diverging() {
    // 70 nesting levels of `List<…>` — instantiation-keyed cycle detection
    // gives each level its own in-progress key, so expansion crosses the
    // depth-64 backstop and the innermost levels stay unexpanded. Both the
    // parameter and return spellings truncate identically, so elaboration
    // still succeeds (the backstop is the pre-21.7.26e fail-safe, not an
    // error path).
    let mut nested = "Nat".to_string();
    for _ in 0..70 {
        nested = format!("List<{nested}>");
    }
    let source = format!(
        "type List<T> = Nil | Cons(T, List<T>)\nfn keep_deep(x: {nested}) -> {nested} {{ x }}\n"
    );
    let defs = elab_ok(&source);
    assert_eq!(defs.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Ctor payload × fresh Tree k=2
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn stored_payload_tree_of_nat() {
    assert_cell_elaborates(
        StoredPosition::CtorPayload,
        GenericAdt::TreeK2,
        ArgKind::BuiltinNat,
    );
}

#[test]
fn stored_payload_tree_of_record() {
    // The `MkCtorIndex(StrMap<CtorBucket>)` two-ctor sum wrapper shape.
    assert_cell_elaborates(
        StoredPosition::CtorPayload,
        GenericAdt::TreeK2,
        ArgKind::Record,
    );
}

#[test]
fn stored_payload_tree_of_sum() {
    assert_cell_elaborates(
        StoredPosition::CtorPayload,
        GenericAdt::TreeK2,
        ArgKind::TwoCtorSum,
    );
}

#[test]
fn stored_payload_tree_of_nested_list() {
    assert_cell_elaborates(
        StoredPosition::CtorPayload,
        GenericAdt::TreeK2,
        ArgKind::NestedListOfRecord,
    );
}
