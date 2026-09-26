//! Tests for the normalization consistency check (ADRs 20.4.26c, 21.7.26e, 21.7.26j).

use super::cross_run::FreshEncodings;
use super::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_check_consistency_simple() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(
        &path,
        "type Color = Red | Green | Blue\nfn main() -> Nat { 0 }",
    )
    .unwrap();
    let result = cmd_check_normalization_consistency(&path, false, 20, false);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_check_consistency_recursive() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "type Wrapper = W(Nat)\nfn main() -> Nat { 0 }").unwrap();
    let result = cmd_check_normalization_consistency(&path, false, 20, false);
    assert_eq!(result, ExitCode::SUCCESS);
}

/// The single-module path elaborates clean standalone files and refuses
/// everything else (parse errors, `mod` declarations) — the refusals are
/// what route multi-module trees to the fallback.
#[test]
fn test_single_module_fresh_encodings_accepts_clean_rejects_broken() {
    let dir = TempDir::new().unwrap();

    let clean = dir.path().join("clean.tg");
    fs::write(&clean, "type Color = Red | Green\nfn main() -> Nat { 0 }").unwrap();
    let fresh = single_module_fresh_encodings(&clean)
        .expect("clean single-module file must elaborate standalone");
    assert!(fresh.encoded_types.contains_key("Color"));

    let broken = dir.path().join("broken.tg");
    fs::write(&broken, "type Color = | | |").unwrap();
    assert!(
        single_module_fresh_encodings(&broken).is_none(),
        "a file with parse errors must be refused, not half-elaborated"
    );

    // A multi-module entry is refused when its cross-module imports can't
    // resolve standalone (`mod` declarations alone are tolerated by the
    // collector; it is the unresolvable `use` that errors — the shape
    // every real multi-module entry file has).
    let multi = dir.path().join("multi.tg");
    fs::write(
        &multi,
        "pub mod util;\nuse util::{Flag, flag_value};\nfn main() -> Nat { flag_value(Flag) }",
    )
    .unwrap();
    assert!(
        single_module_fresh_encodings(&multi).is_none(),
        "a multi-module entry must be refused so the driver fallback runs"
    );
}

/// Multi-module entry files used to hard-error with "failed to
/// re-elaborate", then (21.7.26e) got only a cross-run fallback. They now
/// run the live-elaborator normalization comparison (ADR 21.7.26j): a
/// cross-module ADT normalizes consistently, so the check succeeds.
#[test]
fn test_check_consistency_multi_module_entry() {
    let dir = TempDir::new().unwrap();
    let util = dir.path().join("util.tg");
    fs::write(
        &util,
        "pub type Flag = FlagOff | FlagOn(Nat)\npub fn flag_value(f: Flag) -> Nat {\n    match f { FlagOn(n) => n, FlagOff() => 0 }\n}\n",
    )
    .unwrap();
    let main = dir.path().join("main.tg");
    fs::write(
        &main,
        "pub mod util;\nuse util::{Flag, FlagOn, flag_value};\nfn main() -> Nat { flag_value(FlagOn(3)) }\n",
    )
    .unwrap();
    let result = cmd_check_normalization_consistency(&main, false, 20, false);
    assert_eq!(result, ExitCode::SUCCESS);
}

/// A multi-module tree containing a **record** must not false-fail. Under
/// 21.7.26j the live comparison kept records nominal and *skipped* them;
/// under the per-module oracle (ADR 22.7.26b) the record is genuinely
/// *checked* (source-fresh re-collection reproduces its full Product
/// encoding) and classifies consistent. Either way the check must exit
/// SUCCESS on healthy code — a record must never be a spurious divergence.
#[test]
fn test_check_consistency_multi_module_record_is_checkable_not_divergent() {
    let dir = TempDir::new().unwrap();
    let util = dir.path().join("geom.tg");
    fs::write(
        &util,
        "pub type Point = { x: Nat, y: Nat }\npub fn px(p: Point) -> Nat { p.x }\n",
    )
    .unwrap();
    let main = dir.path().join("main.tg");
    fs::write(
        &main,
        "pub mod geom;\nuse geom::{Point, px};\nfn main() -> Nat { px(Point { x: 1, y: 2 }) }\n",
    )
    .unwrap();
    let result = cmd_check_normalization_consistency(&main, false, 20, false);
    assert_eq!(
        result,
        ExitCode::SUCCESS,
        "a record in a multi-module tree must be checkable, not reported divergent"
    );
}

/// The `--verbose` multi-module path prints a per-type line for every outcome.
/// A tree mixing an ADT and a record exercises the per-module oracle's
/// verbose `✓ consistent (per-module oracle)` branch for both type shapes.
#[test]
fn test_multi_module_verbose_exercises_consistent_and_skip_branches() {
    let dir = TempDir::new().unwrap();
    let util = dir.path().join("m2.tg");
    fs::write(
        &util,
        "pub type Flag = Off | On(Nat)\npub type Pt = { x: Nat }\npub fn f(a: Flag) -> Nat { 0 }\npub fn g(p: Pt) -> Nat { p.x }\n",
    )
    .unwrap();
    let main = dir.path().join("main.tg");
    fs::write(
        &main,
        "pub mod m2;\nuse m2::{Flag, On, Pt, f, g};\nfn main() -> Nat { f(On(1)) }\n",
    )
    .unwrap();
    let result = cmd_check_normalization_consistency(&main, true, 20, false);
    assert_eq!(result, ExitCode::SUCCESS);
}

/// A file with no type declarations hits the empty "no cached encodings" path.
#[test]
fn test_no_type_decls_reports_empty() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty.tg");
    fs::write(&path, "fn main() -> Nat { 0 }\n").unwrap();
    let result = cmd_check_normalization_consistency(&path, false, 20, false);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn type_contains_unexpanded_app_finds_top_and_nested_apps() {
    // A bare `App` (record kept nominal) is detected.
    assert!(type_contains_unexpanded_app(&Type::app("Bucket", vec![])));
    // A fully-expanded structural form has no App.
    assert!(!type_contains_unexpanded_app(&Type::product(
        Type::Nat,
        Type::String
    )));
    // A *nested* App (a generic instantiation `normalize` under-expanded,
    // e.g. `Option<X>` inside a tuple) is detected — the wall-1 case.
    let nested = Type::product(
        Type::TyVar("@ModulePath".to_string()),
        Type::app("Option", vec![Type::TyVar("@ModulePath".to_string())]),
    );
    assert!(type_contains_unexpanded_app(&nested));
    // A μ-expanded generic (no residual App) is clean.
    assert!(!type_contains_unexpanded_app(&Type::Mu(
        "α_List".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::TyVar("α_List".to_string()))
        )),
    )));
}

#[test]
fn type_contains_unexpanded_app_detects_app_in_adt_args_and_fields() {
    // App in an Adt's type_args (empty variants) — the Adt arm's first `||` leg.
    let in_args = Type::Adt("X".to_string(), vec![Type::app("Y", vec![])], vec![]);
    assert!(type_contains_unexpanded_app(&in_args));
    // App in an Adt's variant field (empty type_args) — the second `||` leg.
    let in_field = Type::Adt(
        "X".to_string(),
        vec![],
        vec![("V".to_string(), Type::app("Y", vec![]))],
    );
    assert!(type_contains_unexpanded_app(&in_field));
    // Neither → false.
    let clean = Type::Adt(
        "X".to_string(),
        vec![Type::Nat],
        vec![("V".to_string(), Type::String)],
    );
    assert!(!type_contains_unexpanded_app(&clean));
}

#[test]
fn format_cross_encoding_result_renders_divergent_and_consistent_counts() {
    let tally = NormTally {
        consistent: 5,
        skipped: 0,
        divergent: 2,
    };
    let rendered = format_cross_encoding_result(&tally);
    assert!(rendered.contains("2 divergent"), "got: {rendered}");
    assert!(rendered.contains("5 consistent"), "got: {rendered}");
}

#[test]
fn norm_tally_exit_fails_only_on_divergence() {
    assert_eq!(
        NormTally {
            consistent: 3,
            skipped: 2,
            divergent: 0
        }
        .exit(),
        ExitCode::SUCCESS
    );
    assert_eq!(
        NormTally {
            consistent: 0,
            skipped: 0,
            divergent: 1
        }
        .exit(),
        ExitCode::FAILURE
    );
}

#[test]
fn tally_verdicts_counts_each_outcome() {
    let verdicts = [
        LiveVerdict::Consistent,
        LiveVerdict::Consistent,
        LiveVerdict::Skipped,
        LiveVerdict::Divergent,
    ];
    assert_eq!(
        tally_verdicts(&verdicts),
        NormTally {
            consistent: 2,
            skipped: 1,
            divergent: 1
        }
    );
}

#[test]
fn compare_stored_vs_fresh_tallies_consistent_divergent_and_skipped() {
    let mut encoded: HashMap<String, Type> = HashMap::new();
    encoded.insert("Same".to_string(), Type::Nat);
    encoded.insert("Diff".to_string(), Type::Nat);
    encoded.insert("NoFresh".to_string(), Type::Nat);
    let mut fresh_map: HashMap<String, Type> = HashMap::new();
    fresh_map.insert("Same".to_string(), Type::Nat); // matches → consistent
    fresh_map.insert("Diff".to_string(), Type::String); // differs → divergent
                                                        // "NoFresh" absent → skipped
    let fresh = FreshEncodings {
        label: "test",
        encoded_types: fresh_map,
    };
    let names = ["Diff", "NoFresh", "Same"];
    let tally = compare_stored_vs_fresh(&encoded, &names, &fresh, false);
    assert_eq!(
        tally,
        NormTally {
            consistent: 1,
            skipped: 1,
            divergent: 1
        }
    );
}

#[test]
fn print_header_or_empty_none_when_empty_some_sorted_otherwise() {
    let empty: HashMap<String, Type> = HashMap::new();
    assert_eq!(print_header_or_empty(&empty), None);
    let mut m: HashMap<String, Type> = HashMap::new();
    m.insert("Zebra".to_string(), Type::Nat);
    m.insert("Apple".to_string(), Type::Nat);
    assert_eq!(print_header_or_empty(&m), Some(vec!["Apple", "Zebra"]));
}

/// `type_params` reads a type's declared parameters from the live seeded env
/// (Some for a generic, None for an unknown name).
#[test]
fn type_params_returns_declared_params_and_none_for_unknown() {
    let dir = TempDir::new().unwrap();
    let g = dir.path().join("g.tg");
    fs::write(
        &g,
        "pub type Box2<A, B> = Mk(A, B)\npub fn use_box(b: Box2<Nat, Nat>) -> Nat { 0 }\n",
    )
    .unwrap();
    let main = dir.path().join("main.tg");
    fs::write(
        &main,
        "pub mod g;\nuse g::{Box2, Mk, use_box};\nfn main() -> Nat { 0 }\n",
    )
    .unwrap();
    let mut box2: Option<Vec<String>> = None;
    let mut missing: Option<Vec<String>> = Some(vec!["sentinel".to_string()]);
    let mut ran = false;
    let _ = driver::elaborate_project_with_inspector(&main, false, 20, &mut |normalizer| {
        box2 = normalizer.type_params("Box2");
        missing = normalizer.type_params("NoSuchType");
        ran = true;
    });
    assert!(ran, "the inspector must run");
    assert_eq!(box2, Some(vec!["A".to_string(), "B".to_string()]));
    assert_eq!(missing, None);
}

/// Exercise the cross-run fallback's **success** path directly: a second full
/// elaboration compared against the first is consistent on healthy code. This
/// path is unreachable via `cmd` (the live hook and the fallback share a
/// pipeline, so a hook failure implies a fallback failure), so it is covered
/// here — `cross_run_fallback` is `pub(super)` for exactly this.
#[test]
fn cross_run_fallback_on_healthy_multi_module_succeeds() {
    let dir = TempDir::new().unwrap();
    let util = dir.path().join("u.tg");
    fs::write(
        &util,
        "pub type Flag = Off | On(Nat)\npub fn f(x: Flag) -> Nat { 0 }\n",
    )
    .unwrap();
    let main = dir.path().join("main.tg");
    fs::write(
        &main,
        "pub mod u;\nuse u::{Flag, On, f};\nfn main() -> Nat { f(On(1)) }\n",
    )
    .unwrap();
    assert_eq!(cross_run_fallback(&main, false, 20), ExitCode::SUCCESS);
}

/// A multi-module tree that fails to elaborate must report FAILURE — the live
/// hook returns an error, the internal cross-run fallback re-fails, and the
/// command surfaces it (not a constant-success shim).
#[test]
fn multi_module_elaboration_error_fails() {
    let dir = TempDir::new().unwrap();
    let bad = dir.path().join("bad.tg");
    // `helper2` is undefined → an elaboration error in the imported module.
    fs::write(&bad, "pub fn helper() -> Nat { helper2() }\n").unwrap();
    let main = dir.path().join("main.tg");
    fs::write(
        &main,
        "pub mod bad;\nuse bad::helper;\nfn main() -> Nat { helper() }\n",
    )
    .unwrap();
    let result = cmd_check_normalization_consistency(&main, false, 20, false);
    assert_eq!(result, ExitCode::FAILURE);
}
