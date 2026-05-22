//! Step-by-step type explanation output.
//!
//! Walks a `TypeAst` and emits a numbered, pedagogical explanation
//! of each construct.

use super::type_parser::TypeAst;

/// Explain a parsed type AST with step-by-step output.
pub fn explain_type(ast: &TypeAst) {
    println!("Type Breakdown");
    println!("══════════════");
    println!();
    println!("Input: {ast}");
    println!();

    let mut step = 1;
    walk(ast, &mut step);

    // ADT heuristic summary
    if let Some(name) = infer_adt_name(ast) {
        println!();
        println!("Likely ADT (best-effort heuristic — suggestive, not authoritative):");
        println!(
            "  The μ-binder name `α_{name}` suggests this is a recursive type named `{name}`."
        );
        println!("  Note: multiple ADTs may share the same structural encoding.");
    }

    // Encoding conventions footer
    if has_structural_constructs(ast) {
        println!();
        println!("Encoding conventions:");
        println!("  • Sum (+) represents ADT variants (alternatives)");
        println!("  • Product (×) represents constructor fields (held together)");
        println!("  • Unit represents a nullary constructor (no data)");
        println!("  • μ-binder wraps recursive types; α_<Name> names the ADT");
        println!("  • ∀ introduces a type parameter (generic/polymorphic)");
    }
}

fn walk(ast: &TypeAst, step: &mut usize) {
    match ast {
        TypeAst::Base(name) => walk_base(name, step),
        TypeAst::TyVar(name) => walk_tyvar(name, step),
        TypeAst::Arrow(lhs, rhs) => {
            println!("Step {step} — Arrow type (→):");
            println!("  {ast}");
            println!("  A function type: takes `{lhs}` and returns `{rhs}`.");
            println!();
            *step += 1;
            walk(lhs, step);
            walk(rhs, step);
        }
        TypeAst::Product(lhs, rhs) => {
            walk_product(ast, lhs, rhs, step);
        }
        TypeAst::Sum(lhs, rhs) => {
            walk_sum(ast, lhs, rhs, step);
        }
        TypeAst::Mu(var, body) => {
            walk_mu(var, body, step);
        }
        TypeAst::Forall(var, body) => {
            println!("Step {step} — ∀ (forall) binder:");
            println!("  ∀{var}. <body>");
            println!("  A universally quantified type — polymorphic over `{var}`.");
            println!("  This type works for any choice of `{var}` (like generics).");
            println!();
            *step += 1;
            walk(body, step);
        }
        TypeAst::Error => {
            println!("Step {step} — Type error:");
            println!("  <type error>");
            println!("  A placeholder for a type that could not be determined.");
            println!("  This usually appears when elaboration encountered an error.");
            println!();
            *step += 1;
        }
    }
}

fn walk_base(name: &str, step: &mut usize) {
    println!("Step {step} — Base type:");
    println!("  {name}");
    let desc = match name {
        "Nat" => "Natural numbers (0, 1, 2, ...)",
        "Bool" => "Boolean values (true, false)",
        "Unit" => "The unit type — a type with exactly one value (like void in C)",
        "Void" => "The empty type — a type with no values (uninhabited)",
        "String" => "Text strings",
        "Prop" => "The type of propositions (for theorem proving)",
        _ => "A base type",
    };
    println!("  {desc}");
    println!();
    *step += 1;
}

fn walk_tyvar(name: &str, step: &mut usize) {
    println!("Step {step} — Type variable:");
    if let Some(stripped) = name.strip_prefix('@') {
        println!("  {name}");
        println!("  Named type reference: `{stripped}`");
        println!("  The @-prefix marks a named record or ADT type in Core IR.");
    } else if let Some(adt_name) = name.strip_prefix("α_") {
        // skip "α_" (α is 2 bytes)
        println!("  {name}");
        println!("  Recursive self-reference to type `{adt_name}`.");
        println!("  This variable was bound by a μ (mu) binder above.");
    } else {
        println!("  {name}");
        println!("  A type parameter or variable.");
    }
    println!();
    *step += 1;
}

fn walk_product(ast: &TypeAst, lhs: &TypeAst, rhs: &TypeAst, step: &mut usize) {
    println!("Step {step} — Product type (×):");
    println!("  {ast}");
    println!("  A product holds multiple values together (like a tuple or struct).");
    let fields = collect_product_fields(ast);
    for (i, field) in fields.iter().enumerate() {
        println!("  Field {}: {field}", i + 1);
    }
    println!();
    *step += 1;
    walk(lhs, step);
    walk(rhs, step);
}

fn walk_sum(ast: &TypeAst, lhs: &TypeAst, rhs: &TypeAst, step: &mut usize) {
    println!("Step {step} — Sum type (+):");
    println!("  {ast}");
    println!("  A sum type represents alternatives (like an enum/ADT).");
    let variants = collect_sum_variants(ast);
    println!("  This type has {} variants:", variants.len());
    for (i, variant) in variants.iter().enumerate() {
        let label = if variants.len() == 2 {
            if i == 0 {
                "Left".to_string()
            } else {
                "Right".to_string()
            }
        } else {
            format!("Variant {}", i + 1)
        };
        let desc = describe_variant(variant);
        println!("    {label}:  {variant}  ({desc})");
    }
    println!();
    *step += 1;
    walk(lhs, step);
    walk(rhs, step);
}

fn walk_mu(var: &str, body: &TypeAst, step: &mut usize) {
    println!("Step {step} — μ (mu) binder:");
    println!("  μ{var}. <body>");
    println!("  This is a recursive type. The variable `{var}` refers back to");
    println!("  the type itself, enabling self-reference (like a linked list");
    println!("  pointing to another list node).");
    if let Some(adt_name) = var.strip_prefix("α_") {
        println!("  The binder name suggests this encodes an ADT named `{adt_name}`.");
    }
    println!();
    *step += 1;
    walk(body, step);
}

/// Collect the top-level sum variants (flattening left-associated sums).
fn collect_sum_variants(ast: &TypeAst) -> Vec<&TypeAst> {
    match ast {
        TypeAst::Sum(lhs, rhs) => {
            let mut variants = collect_sum_variants(lhs);
            variants.extend(collect_sum_variants(rhs));
            variants
        }
        _ => vec![ast],
    }
}

/// Collect the top-level product fields (flattening right-associated products).
fn collect_product_fields(ast: &TypeAst) -> Vec<&TypeAst> {
    match ast {
        TypeAst::Product(lhs, rhs) => {
            let mut fields = vec![lhs.as_ref()];
            fields.extend(collect_product_fields(rhs));
            fields
        }
        _ => vec![ast],
    }
}

/// Brief description of a variant for the summary.
fn describe_variant(ast: &TypeAst) -> &'static str {
    match ast {
        TypeAst::Base(name) if name == "Unit" => "nullary constructor — no data",
        TypeAst::Product(_, _) => "constructor with multiple fields",
        TypeAst::Base(_) => "constructor with one field",
        TypeAst::TyVar(v) if v.starts_with("α_") => "recursive self-reference",
        TypeAst::TyVar(_) => "type variable",
        TypeAst::Arrow(_, _) => "function type",
        TypeAst::Sum(_, _) => "nested sum (multi-variant)",
        TypeAst::Mu(_, _) => "recursive type",
        TypeAst::Forall(_, _) => "polymorphic type",
        TypeAst::Error => "type error",
    }
}

/// Infer the ADT name from a μ-binder using the α_ convention.
fn infer_adt_name(ast: &TypeAst) -> Option<&str> {
    match ast {
        TypeAst::Mu(var, _) => var.strip_prefix("α_"),
        _ => None,
    }
}

/// Check if the type uses structural constructs worth explaining in footer.
fn has_structural_constructs(ast: &TypeAst) -> bool {
    match ast {
        TypeAst::Sum(_, _) | TypeAst::Product(_, _) | TypeAst::Mu(_, _) | TypeAst::Forall(_, _) => {
            true
        }
        TypeAst::Arrow(a, b) => has_structural_constructs(a) || has_structural_constructs(b),
        _ => false,
    }
}
