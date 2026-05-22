//! Fold/unfold consistency check for all ADTs (ADR 21.4.26b).
//!
//! Validates that every ADT in a project has consistent fold/unfold
//! treatment across SCC membership, μ-binder encoding, and Core IR.

mod output;

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;
use crate::fold_analysis;

/// Run fold/unfold consistency check across all ADTs in a file.
pub fn cmd_check_fold_consistency(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    json: bool,
) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    if project.adt_types.is_empty() {
        if json {
            println!("{{\"adts\": [], \"status\": \"pass\"}}");
        } else {
            println!("No ADTs found.");
        }
        return ExitCode::SUCCESS;
    }

    let mut adt_names: Vec<&String> = project.adt_types.keys().collect();
    adt_names.sort();

    let results: Vec<AdtFoldResult> = adt_names
        .iter()
        .map(|name| analyze_adt(name, &project))
        .collect();

    if json {
        output::print_json(&results);
    } else {
        output::print_table(&results, verbose);
    }

    let all_consistent = results.iter().all(|r| r.consistent);
    if all_consistent {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Per-ADT consistency result.
pub(crate) struct AdtFoldResult {
    pub name: String,
    pub is_recursive: bool,
    pub has_mu_binder: bool,
    pub fold_sites: usize,
    pub unfold_sites: usize,
    pub consistent: bool,
    pub scc_group: Option<Vec<String>>,
    pub disagreements: Vec<String>,
}

/// Analyze a single ADT for fold/unfold consistency.
fn analyze_adt(name: &str, project: &driver::ProjectOutput) -> AdtFoldResult {
    let in_scc = project.mutual_recursion_groups.contains_key(name);
    let has_mu_origin = project
        .type_provenance
        .mu_origins
        .values()
        .any(|o| o.adt_name == name);
    let is_recursive = in_scc || has_mu_origin;

    let has_mu_binder = project
        .encoded_types
        .get(name)
        .map(|ty| fold_analysis::encoding_has_mu_binder(ty, name))
        .unwrap_or(false);

    let mu_var = format!("α_{name}");
    let fold_sites = fold_analysis::count_fold_unfold_sites(&project.defs, &mu_var, true);
    let unfold_sites = fold_analysis::count_fold_unfold_sites(&project.defs, &mu_var, false);

    let scc_group = project.mutual_recursion_groups.get(name).cloned();
    let disagreements =
        compute_disagreements(is_recursive, has_mu_binder, fold_sites, unfold_sites);
    let consistent = disagreements.is_empty();

    AdtFoldResult {
        name: name.to_string(),
        is_recursive,
        has_mu_binder,
        fold_sites,
        unfold_sites,
        consistent,
        scc_group,
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
            ("encoding has no μ-binder", !has_mu_binder),
            ("no Fold nodes in Core IR", fold_sites == 0),
            ("no Unfold nodes in Core IR", unfold_sites == 0),
        ]
    } else {
        &[
            ("encoding has μ-binder", has_mu_binder),
            ("Fold nodes found in Core IR", fold_sites > 0),
            ("Unfold nodes found in Core IR", unfold_sites > 0),
        ]
    };
    checks
        .iter()
        .filter(|(_, cond)| *cond)
        .map(|(msg, _)| (*msg).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tungsten_core::Type;

    #[test]
    fn test_consistent_non_recursive_adt() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(
            &path,
            "pub type Color = Red | Green | Blue\nfn main() -> Nat { 0 }",
        )
        .unwrap();
        let result = cmd_check_fold_consistency(&path, false, 20, false);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_consistent_recursive_adt() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(
            &path,
            "type List = Nil | Cons(Nat, List)\n\
             fn main() -> Nat { 0 }",
        )
        .unwrap();
        let result = cmd_check_fold_consistency(&path, false, 20, false);
        assert!(
            result != ExitCode::from(2),
            "should not be a usage/internal error"
        );
    }

    #[test]
    fn test_no_adts() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(&path, "fn main() -> Nat { 0 }").unwrap();
        let result = cmd_check_fold_consistency(&path, false, 20, false);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_json_output() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(
            &path,
            "pub type Color = Red | Green | Blue\nfn main() -> Nat { 0 }",
        )
        .unwrap();
        let result = cmd_check_fold_consistency(&path, false, 20, true);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_mixed_adt_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(
            &path,
            "type Color = Red | Green | Blue\n\
             type List = Nil | Cons(Nat, List)\n\
             fn main() -> Nat { 0 }",
        )
        .unwrap();
        let result = cmd_check_fold_consistency(&path, false, 20, false);
        assert!(
            result != ExitCode::from(2),
            "should not be a usage/internal error"
        );
    }

    #[test]
    fn test_record_type_ignored() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(
            &path,
            "type Point = { x: Nat, y: Nat }\nfn main() -> Nat { 0 }",
        )
        .unwrap();
        let result = cmd_check_fold_consistency(&path, false, 20, false);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_json_output_with_recursive() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(
            &path,
            "type List = Nil | Cons(Nat, List)\n\
             fn main() -> Nat { 0 }",
        )
        .unwrap();
        let result = cmd_check_fold_consistency(&path, false, 20, true);
        assert!(
            result != ExitCode::from(2),
            "should not be a usage/internal error"
        );
    }

    #[test]
    fn test_bad_file_returns_exit_2() {
        let result =
            cmd_check_fold_consistency(&PathBuf::from("/nonexistent/file.tg"), false, 20, false);
        assert_eq!(result, ExitCode::from(2));
    }

    // ═══════════════════════════════════════════════════════════════════
    // Unit tests for helpers
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn test_encoding_has_mu_binder_true() {
        let ty = Type::Mu("α_List".to_string(), Box::new(Type::Nat));
        assert!(fold_analysis::encoding_has_mu_binder(&ty, "List"));
    }

    #[test]
    fn test_encoding_has_mu_binder_false() {
        assert!(!fold_analysis::encoding_has_mu_binder(&Type::Nat, "List"));
    }

    #[test]
    fn test_encoding_has_mu_binder_wrong_name() {
        let ty = Type::Mu("α_Tree".to_string(), Box::new(Type::Nat));
        assert!(!fold_analysis::encoding_has_mu_binder(&ty, "List"));
    }

    #[test]
    fn test_type_has_mu_var_direct() {
        assert!(fold_analysis::type_has_mu_var(
            &Type::TyVar("α_X".to_string()),
            "α_X"
        ));
        assert!(!fold_analysis::type_has_mu_var(
            &Type::TyVar("α_X".to_string()),
            "α_Y"
        ));
    }

    #[test]
    fn test_type_has_mu_var_nested() {
        let ty = Type::Arrow(
            Box::new(Type::Nat),
            Box::new(Type::TyVar("α_List".to_string())),
        );
        assert!(fold_analysis::type_has_mu_var(&ty, "α_List"));
        assert!(!fold_analysis::type_has_mu_var(&ty, "α_Other"));
    }

    #[test]
    fn test_count_in_term_fold() {
        use tungsten_core::Term;
        let mu_ty = Type::Mu("α_List".to_string(), Box::new(Type::Nat));
        let term = Term::Fold(mu_ty, Box::new(Term::Zero));
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", true), 1);
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", false), 0);
    }

    #[test]
    fn test_count_in_term_unfold() {
        use tungsten_core::Term;
        let mu_ty = Type::Mu("α_List".to_string(), Box::new(Type::Nat));
        let term = Term::Unfold(mu_ty, Box::new(Term::Zero));
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", false), 1);
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", true), 0);
    }

    #[test]
    fn test_count_in_term_no_matches() {
        use tungsten_core::Term;
        let term = Term::App(Box::new(Term::Zero), Box::new(Term::True));
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", true), 0);
        assert_eq!(fold_analysis::count_in_term(&term, "α_List", false), 0);
    }

    #[test]
    fn test_count_in_term_nested_in_let() {
        use tungsten_core::Term;
        let mu_ty = Type::Mu("α_List".to_string(), Box::new(Type::Nat));
        let fold_term = Term::Fold(mu_ty, Box::new(Term::Zero));
        let let_term = Term::Let(
            "x".to_string(),
            Type::Nat,
            Box::new(fold_term),
            Box::new(Term::Zero),
        );
        assert_eq!(fold_analysis::count_in_term(&let_term, "α_List", true), 1);
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
        // Recursive ADT with no μ-binder, no folds, no unfolds → 3 disagreements
        let d = compute_disagreements(true, false, 0, 0);
        assert_eq!(d.len(), 3);
        assert!(d[0].contains("no μ-binder"));
        assert!(d[1].contains("no Fold"));
        assert!(d[2].contains("no Unfold"));
    }

    #[test]
    fn test_disagreements_non_recursive_all_three() {
        // Non-recursive ADT with μ-binder + fold + unfold → 3 disagreements
        let d = compute_disagreements(false, true, 2, 1);
        assert_eq!(d.len(), 3);
        assert!(d[0].contains("has μ-binder"));
        assert!(d[1].contains("Fold"));
        assert!(d[2].contains("Unfold"));
    }
}
