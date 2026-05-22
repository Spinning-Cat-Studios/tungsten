//! Structural tree-diff of two type representations (ADR 20.4.26c).
//!
//! Compares two type names from the same project and displays an inline diff
//! showing where the type trees diverge, with +/- markers.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;
use tungsten_core::Type;

/// Run the diff-types command: compare the encoded types of two named types.
pub fn cmd_diff_types(
    type_a: &str,
    type_b: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Look up encoded types
    let enc_a = project.encoded_types.get(type_a);
    let enc_b = project.encoded_types.get(type_b);

    // Check existence
    if enc_a.is_none() && enc_b.is_none() {
        eprintln!("Neither type has a cached encoding: {type_a}, {type_b}");
        print_available_types(&project);
        return ExitCode::FAILURE;
    }
    if enc_a.is_none() {
        eprintln!("Type '{type_a}' has no cached encoding (may be parameterized or not found)");
        print_available_types(&project);
        return ExitCode::FAILURE;
    }
    if enc_b.is_none() {
        eprintln!("Type '{type_b}' has no cached encoding (may be parameterized or not found)");
        print_available_types(&project);
        return ExitCode::FAILURE;
    }

    let ty_a = enc_a.unwrap();
    let ty_b = enc_b.unwrap();

    println!("Comparing: {type_a} vs {type_b}");
    println!("{}", "═".repeat(12 + type_a.len() + type_b.len()));
    println!();

    if ty_a == ty_b {
        println!("Types are structurally identical.");
        println!();
        println!("Encoding:");
        println!("  {ty_a}");
        return ExitCode::SUCCESS;
    }

    // Types differ — show tree diff
    let diffs = diff_types(ty_a, ty_b, 0);
    println!("Diff ({} divergence(s)):", diffs.len());
    println!();

    // Print the unified diff view
    print_type_diff(ty_a, ty_b, 1);
    println!();

    // Summary
    println!("Result: {} node divergence(s) found", diffs.len());

    ExitCode::from(1) // nonzero = types differ
}

/// A single point of divergence between two type trees.
struct TypeDiff {
    depth: usize,
    description: String,
}

/// Recursively diff two type trees, collecting divergence points.
fn diff_types(a: &Type, b: &Type, depth: usize) -> Vec<TypeDiff> {
    if a == b {
        return Vec::new();
    }

    match (a, b) {
        // Same constructor, different children
        (Type::Arrow(a1, a2), Type::Arrow(b1, b2))
        | (Type::Product(a1, a2), Type::Product(b1, b2))
        | (Type::Sum(a1, a2), Type::Sum(b1, b2)) => diff_binary(a1, b1, a2, b2, depth),
        (Type::Mu(va, ba), Type::Mu(vb, bb)) => diff_binder(
            &BinderDiffCtx {
                kind: "Mu",
                var_a: va,
                var_b: vb,
                body_a: ba,
                body_b: bb,
            },
            depth,
        ),
        (Type::Forall(va, ba), Type::Forall(vb, bb)) => diff_binder(
            &BinderDiffCtx {
                kind: "Forall",
                var_a: va,
                var_b: vb,
                body_a: ba,
                body_b: bb,
            },
            depth,
        ),
        (Type::Ptr(a), Type::Ptr(b)) | (Type::Ref(a), Type::Ref(b)) => diff_types(a, b, depth + 1),
        (Type::App(na, aa), Type::App(nb, ab)) if na == nb && aa.len() == ab.len() => {
            diff_type_lists(aa, ab, depth)
        }
        (Type::Adt(na, ta, va), Type::Adt(nb, tb, vb))
            if na == nb && ta.len() == tb.len() && va.len() == vb.len() =>
        {
            diff_adt(ta, tb, va, vb, depth)
        }
        // Different top-level constructors
        _ => {
            vec![TypeDiff {
                depth,
                description: format!("- {}\n+ {}", a.display_detailed(), b.display_detailed()),
            }]
        }
    }
}

/// Diff two binary type constructors (Arrow, Product, Sum).
fn diff_binary(a1: &Type, b1: &Type, a2: &Type, b2: &Type, depth: usize) -> Vec<TypeDiff> {
    let mut diffs = diff_types(a1, b1, depth + 1);
    diffs.extend(diff_types(a2, b2, depth + 1));
    diffs
}

/// Context for diffing a binding type constructor (Mu, Forall).
struct BinderDiffCtx<'a> {
    kind: &'a str,
    var_a: &'a str,
    var_b: &'a str,
    body_a: &'a Type,
    body_b: &'a Type,
}

/// Diff two binding type constructors (Mu, Forall).
fn diff_binder(ctx: &BinderDiffCtx, depth: usize) -> Vec<TypeDiff> {
    let mut diffs = Vec::new();
    if ctx.var_a != ctx.var_b {
        diffs.push(TypeDiff {
            depth,
            description: format!("{} binder: {} vs {}", ctx.kind, ctx.var_a, ctx.var_b),
        });
    }
    diffs.extend(diff_types(ctx.body_a, ctx.body_b, depth + 1));
    diffs
}

/// Diff parallel lists of types (for App args or Adt type args).
fn diff_type_lists(aa: &[Type], ab: &[Type], depth: usize) -> Vec<TypeDiff> {
    aa.iter()
        .zip(ab.iter())
        .flat_map(|(ai, bi)| diff_types(ai, bi, depth + 1))
        .collect()
}

/// Diff two Adt types with matching names and arities.
fn diff_adt(
    ta: &[Type],
    tb: &[Type],
    va: &[(String, Type)],
    vb: &[(String, Type)],
    depth: usize,
) -> Vec<TypeDiff> {
    let mut diffs = diff_type_lists(ta, tb, depth);
    for ((cn_a, ct_a), (cn_b, ct_b)) in va.iter().zip(vb.iter()) {
        if cn_a != cn_b {
            diffs.push(TypeDiff {
                depth: depth + 1,
                description: format!("Adt variant name: {cn_a} vs {cn_b}"),
            });
        }
        diffs.extend(diff_types(ct_a, ct_b, depth + 2));
    }
    diffs
}

/// Pretty-print a side-by-side type diff with indentation.
fn print_type_diff(a: &Type, b: &Type, indent: usize) {
    let prefix = "  ".repeat(indent);

    if a == b {
        println!("{prefix}{}", a.display_detailed());
        return;
    }

    match (a, b) {
        (Type::Arrow(a1, a2), Type::Arrow(b1, b2)) => {
            println!("{prefix}Arrow(");
            print_type_diff(a1, b1, indent + 1);
            println!("{prefix}  ,");
            print_type_diff(a2, b2, indent + 1);
            println!("{prefix})");
        }
        (Type::Product(a1, a2), Type::Product(b1, b2)) => {
            println!("{prefix}Product(");
            print_type_diff(a1, b1, indent + 1);
            println!("{prefix}  ,");
            print_type_diff(a2, b2, indent + 1);
            println!("{prefix})");
        }
        (Type::Sum(a1, a2), Type::Sum(b1, b2)) => {
            println!("{prefix}Sum(");
            print_type_diff(a1, b1, indent + 1);
            println!("{prefix}  ,");
            print_type_diff(a2, b2, indent + 1);
            println!("{prefix})");
        }
        (Type::Mu(va, ba), Type::Mu(vb, bb)) => {
            if va == vb {
                println!("{prefix}Mu({va},");
            } else {
                println!("{prefix}- Mu({va},");
                println!("{prefix}+ Mu({vb},");
            }
            print_type_diff(ba, bb, indent + 1);
            println!("{prefix})");
        }
        (Type::Forall(va, ba), Type::Forall(vb, bb)) => {
            if va == vb {
                println!("{prefix}Forall({va},");
            } else {
                println!("{prefix}- Forall({va},");
                println!("{prefix}+ Forall({vb},");
            }
            print_type_diff(ba, bb, indent + 1);
            println!("{prefix})");
        }
        (Type::Ptr(a), Type::Ptr(b)) => {
            println!("{prefix}Ptr(");
            print_type_diff(a, b, indent + 1);
            println!("{prefix})");
        }
        (Type::Ref(a), Type::Ref(b)) => {
            println!("{prefix}Ref(");
            print_type_diff(a, b, indent + 1);
            println!("{prefix})");
        }
        _ => {
            // Leaf divergence
            println!("{prefix}- {}", a.display_detailed());
            println!("{prefix}+ {}", b.display_detailed());
        }
    }
}

fn print_available_types(project: &driver::ProjectOutput) {
    let available: Vec<&str> = project.encoded_types.keys().map(|s| s.as_str()).collect();
    if !available.is_empty() {
        let mut sorted = available;
        sorted.sort();
        eprintln!("Types with cached encodings: {}", sorted.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_identical_types() {
        let ty = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool));
        let diffs = diff_types(&ty, &ty, 0);
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_diff_different_leaf() {
        let a = Type::Nat;
        let b = Type::Bool;
        let diffs = diff_types(&a, &b, 0);
        assert_eq!(diffs.len(), 1);
    }

    #[test]
    fn test_diff_nested_difference() {
        let a = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool));
        let b = Type::Arrow(Box::new(Type::Nat), Box::new(Type::String));
        let diffs = diff_types(&a, &b, 0);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].depth, 1); // Difference is in the return type
    }

    #[test]
    fn test_diff_mu_binder_difference() {
        let a = Type::Mu("α_A".to_string(), Box::new(Type::Unit));
        let b = Type::Mu("α_B".to_string(), Box::new(Type::Unit));
        let diffs = diff_types(&a, &b, 0);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].description.contains("Mu binder"));
    }

    #[test]
    fn test_diff_sum_children() {
        let a = Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool));
        let b = Type::Sum(Box::new(Type::String), Box::new(Type::Bool));
        let diffs = diff_types(&a, &b, 0);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].depth, 1);
    }

    #[test]
    fn test_diff_cmd_exit_success_identical() {
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(&path, "type A = X()\ntype B = Y()\nfn main() -> Nat { 0 }").unwrap();
        // Both are single-constructor ADTs encoding to Unit — identical
        let result = cmd_diff_types("A", "B", &path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }
}
