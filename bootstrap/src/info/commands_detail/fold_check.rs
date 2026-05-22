//! Fold/unfold consistency check for `info adt --check-fold` (ADR 21.4.26b).

use std::process::ExitCode;

use tungsten_bootstrap::driver::ProjectOutput;
use tungsten_bootstrap::fold_analysis;

/// Fold check analysis result for a single ADT.
struct FoldCheckResult {
    _is_recursive: bool,
    recursion_label: String,
    has_mu_binder: bool,
    fold_sites: usize,
    unfold_sites: usize,
    disagreements: Vec<String>,
}

/// Analyze a single ADT for fold/unfold consistency.
fn analyze_fold_check(name: &str, project: &ProjectOutput) -> FoldCheckResult {
    let in_scc = project.mutual_recursion_groups.contains_key(name);
    let scc_group = project.mutual_recursion_groups.get(name);
    let has_mu_origin = project
        .type_provenance
        .mu_origins
        .values()
        .any(|o| o.adt_name == name);
    let is_recursive = in_scc || has_mu_origin;

    let recursion_label = if let Some(group) = scc_group {
        format!("true  (mutual group: {{{}}})", group.join(", "))
    } else if has_mu_origin {
        "true  (self-recursive)".to_string()
    } else {
        "false".to_string()
    };

    let has_mu_binder = project
        .encoded_types
        .get(name)
        .is_some_and(|ty| fold_analysis::encoding_has_mu_binder(ty, name));

    let mu_var = format!("α_{name}");
    let fold_sites = fold_analysis::count_fold_unfold_sites(&project.defs, &mu_var, true);
    let unfold_sites = fold_analysis::count_fold_unfold_sites(&project.defs, &mu_var, false);

    let disagreements =
        compute_disagreements(is_recursive, has_mu_binder, fold_sites, unfold_sites);

    FoldCheckResult {
        _is_recursive: is_recursive,
        recursion_label,
        has_mu_binder,
        fold_sites,
        unfold_sites,
        disagreements,
    }
}

/// Compute disagreements between recursion status and fold/unfold evidence.
fn compute_disagreements(
    is_recursive: bool,
    has_mu_binder: bool,
    fold_sites: usize,
    unfold_sites: usize,
) -> Vec<String> {
    let checks: &[(&str, bool)] = if is_recursive {
        &[
            (
                "SCC says recursive, but encoding has no μ-binder",
                !has_mu_binder,
            ),
            (
                "SCC says recursive, but no Fold nodes in Core IR",
                fold_sites == 0,
            ),
            (
                "SCC says recursive, but no Unfold nodes in Core IR",
                unfold_sites == 0,
            ),
        ]
    } else {
        &[
            ("not recursive, but encoding has μ-binder", has_mu_binder),
            (
                "not recursive, but Fold nodes found in Core IR",
                fold_sites > 0,
            ),
            (
                "not recursive, but Unfold nodes found in Core IR",
                unfold_sites > 0,
            ),
        ]
    };
    checks
        .iter()
        .filter(|(_, cond)| *cond)
        .map(|(msg, _)| format!("    → {msg}"))
        .collect()
}

/// Format a site count as "true  (N site(s))" or "false  (none)".
fn format_site_count(count: usize) -> String {
    if count > 0 {
        format!("true  ({count} site(s))")
    } else {
        "false  (none)".to_string()
    }
}

/// Print fold/unfold consistency report for a single ADT.
pub fn print_fold_check(name: &str, project: &ProjectOutput) -> ExitCode {
    let r = analyze_fold_check(name, project);

    println!();
    println!("Fold/Unfold Check:");
    println!("──────────────────");
    println!("  Recursive:        {}", r.recursion_label);
    println!(
        "  Has μ-binder:     {}",
        if r.has_mu_binder { "true" } else { "false" }
    );
    println!("  Fold emitted:     {}", format_site_count(r.fold_sites));
    println!("  Unfold emitted:   {}", format_site_count(r.unfold_sites));
    println!();

    if r.disagreements.is_empty() {
        println!("  Status:           ✓ CONSISTENT");
        ExitCode::SUCCESS
    } else {
        println!("  Status:           ✗ INCONSISTENT");
        for d in &r.disagreements {
            println!("{d}");
        }
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_core::Type;

    // ═══════════════════════════════════════════════════════════════════
    // encoding_has_mu_binder (via shared module)
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn test_encoding_has_mu_binder_matching() {
        let ty = Type::Mu("α_List".to_string(), Box::new(Type::Nat));
        assert!(fold_analysis::encoding_has_mu_binder(&ty, "List"));
    }

    #[test]
    fn test_encoding_has_mu_binder_wrong_name() {
        let ty = Type::Mu("α_Tree".to_string(), Box::new(Type::Nat));
        assert!(!fold_analysis::encoding_has_mu_binder(&ty, "List"));
    }

    #[test]
    fn test_encoding_has_mu_binder_non_mu_type() {
        assert!(!fold_analysis::encoding_has_mu_binder(&Type::Nat, "List"));
        assert!(!fold_analysis::encoding_has_mu_binder(&Type::Bool, "List"));
    }

    #[test]
    fn test_encoding_has_mu_binder_nested_mu() {
        let ty = Type::Mu(
            "α_Other".to_string(),
            Box::new(Type::Mu("α_List".to_string(), Box::new(Type::Nat))),
        );
        assert!(!fold_analysis::encoding_has_mu_binder(&ty, "List"));
        assert!(fold_analysis::encoding_has_mu_binder(&ty, "Other"));
    }

    // ═══════════════════════════════════════════════════════════════════
    // type_has_mu_var (via shared module)
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn test_type_has_mu_var_direct_tyvar() {
        let ty = Type::TyVar("α_List".to_string());
        assert!(fold_analysis::type_has_mu_var(&ty, "α_List"));
        assert!(!fold_analysis::type_has_mu_var(&ty, "α_Tree"));
    }

    #[test]
    fn test_type_has_mu_var_in_mu_binder() {
        let ty = Type::Mu("α_List".to_string(), Box::new(Type::Nat));
        assert!(fold_analysis::type_has_mu_var(&ty, "α_List"));
    }

    #[test]
    fn test_type_has_mu_var_nested_in_arrow() {
        let ty = Type::Arrow(
            Box::new(Type::Nat),
            Box::new(Type::TyVar("α_List".to_string())),
        );
        assert!(fold_analysis::type_has_mu_var(&ty, "α_List"));
        assert!(!fold_analysis::type_has_mu_var(&ty, "α_Tree"));
    }

    #[test]
    fn test_type_has_mu_var_nested_in_product() {
        let ty = Type::Product(
            Box::new(Type::TyVar("α_A".to_string())),
            Box::new(Type::TyVar("α_B".to_string())),
        );
        assert!(fold_analysis::type_has_mu_var(&ty, "α_A"));
        assert!(fold_analysis::type_has_mu_var(&ty, "α_B"));
        assert!(!fold_analysis::type_has_mu_var(&ty, "α_C"));
    }

    #[test]
    fn test_type_has_mu_var_simple_types() {
        assert!(!fold_analysis::type_has_mu_var(&Type::Nat, "α_List"));
        assert!(!fold_analysis::type_has_mu_var(&Type::Bool, "α_List"));
        assert!(!fold_analysis::type_has_mu_var(&Type::Unit, "α_List"));
        assert!(!fold_analysis::type_has_mu_var(&Type::String, "α_List"));
    }

    // ═══════════════════════════════════════════════════════════════════
    // count_in_term (via shared module)
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn test_count_in_term_fold_matching() {
        use tungsten_core::Term;
        let mu_ty = Type::Mu("α_List".to_string(), Box::new(Type::Nat));
        let term = Term::Fold(mu_ty, Box::new(Term::Zero));
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", true), 1);
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", false), 0);
    }

    #[test]
    fn test_count_in_term_unfold_matching() {
        use tungsten_core::Term;
        let mu_ty = Type::Mu("α_List".to_string(), Box::new(Type::Nat));
        let term = Term::Unfold(mu_ty, Box::new(Term::Zero));
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", false), 1);
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", true), 0);
    }

    #[test]
    fn test_count_in_term_fold_wrong_mu_var() {
        use tungsten_core::Term;
        let mu_ty = Type::Mu("α_Tree".to_string(), Box::new(Type::Nat));
        let term = Term::Fold(mu_ty, Box::new(Term::Zero));
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", true), 0);
    }

    #[test]
    fn test_count_in_term_nested_folds() {
        use tungsten_core::Term;
        let mu_ty = Type::Mu("α_List".to_string(), Box::new(Type::Nat));
        let inner = Term::Fold(mu_ty.clone(), Box::new(Term::Zero));
        let outer = Term::Fold(mu_ty, Box::new(inner));
        assert_eq!(fold_analysis::count_in_term(&outer, "α_List", true), 2);
    }

    #[test]
    fn test_count_in_term_fold_inside_app() {
        use tungsten_core::Term;
        let mu_ty = Type::Mu("α_List".to_string(), Box::new(Type::Nat));
        let fold_term = Term::Fold(mu_ty, Box::new(Term::Zero));
        let app = Term::App(Box::new(Term::Zero), Box::new(fold_term));
        assert_eq!(fold_analysis::count_in_term(&app, "α_List", true), 1);
    }

    #[test]
    fn test_count_in_term_no_fold_unfold() {
        use tungsten_core::Term;
        let term = Term::App(Box::new(Term::Zero), Box::new(Term::True));
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", true), 0);
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", false), 0);
    }

    #[test]
    fn test_count_in_term_spanned_passthrough() {
        use tungsten_core::Term;
        let mu_ty = Type::Mu("α_List".to_string(), Box::new(Type::Nat));
        let fold_term = Term::Fold(mu_ty, Box::new(Term::Zero));
        let spanned = Term::Spanned(
            Box::new(fold_term),
            tungsten_core::terms::TermSpan::new(0, 0),
        );
        assert_eq!(fold_analysis::count_in_term(&spanned, "α_List", true), 1);
    }

    // ═══════════════════════════════════════════════════════════════════
    // compute_disagreements
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn test_disagreements_consistent_recursive() {
        let d = compute_disagreements(true, true, 1, 1);
        assert!(d.is_empty());
    }

    #[test]
    fn test_disagreements_consistent_non_recursive() {
        let d = compute_disagreements(false, false, 0, 0);
        assert!(d.is_empty());
    }

    #[test]
    fn test_disagreements_recursive_missing_mu() {
        let d = compute_disagreements(true, false, 1, 1);
        assert_eq!(d.len(), 1);
        assert!(d[0].contains("no μ-binder"));
    }

    #[test]
    fn test_disagreements_non_recursive_with_fold() {
        let d = compute_disagreements(false, false, 3, 0);
        assert_eq!(d.len(), 1);
        assert!(d[0].contains("Fold"));
    }

    #[test]
    fn test_disagreements_recursive_all_three() {
        let d = compute_disagreements(true, false, 0, 0);
        assert_eq!(d.len(), 3);
        assert!(d[0].contains("no μ-binder"));
        assert!(d[1].contains("no Fold"));
        assert!(d[2].contains("no Unfold"));
    }

    #[test]
    fn test_disagreements_non_recursive_all_three() {
        let d = compute_disagreements(false, true, 2, 1);
        assert_eq!(d.len(), 3);
        assert!(d[0].contains("has μ-binder"));
        assert!(d[1].contains("Fold"));
        assert!(d[2].contains("Unfold"));
    }

    // ═══════════════════════════════════════════════════════════════════
    // format_site_count
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn test_format_site_count_zero() {
        assert_eq!(format_site_count(0), "false  (none)");
    }

    #[test]
    fn test_format_site_count_positive() {
        assert!(format_site_count(3).contains("3 site(s)"));
    }
}
