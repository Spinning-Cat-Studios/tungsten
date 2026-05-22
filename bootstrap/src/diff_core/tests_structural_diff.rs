//! Structural diff tests for `tungsten diff-core`.

#[cfg(test)]
mod structural_diff_tests {
    use crate::diff_core::sexpr::{format_path, parse_sexpr, structural_diff};

    #[test]
    fn identical_atoms_no_divergence() {
        let a = parse_sexpr("Nat");
        let b = parse_sexpr("Nat");
        let divs = structural_diff(&a, &b, 10);
        assert!(divs.is_empty());
    }

    #[test]
    fn identical_complex_no_divergence() {
        let a = parse_sexpr("μα_Expr. ((Unit + α_Expr) + (α_Expr × Nat))");
        let b = parse_sexpr("μα_Expr. ((Unit + α_Expr) + (α_Expr × Nat))");
        let divs = structural_diff(&a, &b, 10);
        assert!(divs.is_empty());
    }

    #[test]
    fn atom_mismatch() {
        let a = parse_sexpr("Nat");
        let b = parse_sexpr("Bool");
        let divs = structural_diff(&a, &b, 10);
        assert_eq!(divs.len(), 1);
        assert_eq!(divs[0].left, "Nat");
        assert_eq!(divs[0].right, "Bool");
        assert_eq!(divs[0].depth, 1);
    }

    #[test]
    fn nested_type_divergence() {
        let a = parse_sexpr("(Nat → Nat)");
        let b = parse_sexpr("(Nat → Bool)");
        let divs = structural_diff(&a, &b, 10);
        assert_eq!(divs.len(), 1);
        assert_eq!(divs[0].left, "Nat");
        assert_eq!(divs[0].right, "Bool");
        // depth 2: top-level [0] → paren child [2]
        assert_eq!(divs[0].depth, 2);
    }

    #[test]
    fn deep_mu_type_divergence() {
        let a = parse_sexpr("μα_Expr. ((Unit + α_Expr) + (α_Expr × Nat))");
        let b = parse_sexpr("μα_Expr. ((Unit + α_Expr) + (α_Expr × Bool))");
        let divs = structural_diff(&a, &b, 10);
        assert_eq!(divs.len(), 1);
        assert_eq!(divs[0].left, "Nat");
        assert_eq!(divs[0].right, "Bool");
        // Deeper nesting: top [1] → paren [2] → paren [2]
        assert!(divs[0].depth >= 3);
    }

    #[test]
    fn multiple_divergences() {
        let a = parse_sexpr("(Nat → Bool)");
        let b = parse_sexpr("(String → Unit)");
        let divs = structural_diff(&a, &b, 10);
        assert_eq!(divs.len(), 2);
        assert_eq!(divs[0].left, "Nat");
        assert_eq!(divs[0].right, "String");
        assert_eq!(divs[1].left, "Bool");
        assert_eq!(divs[1].right, "Unit");
    }

    #[test]
    fn different_child_count() {
        let a = parse_sexpr("(f x)");
        let b = parse_sexpr("(f x y)");
        let divs = structural_diff(&a, &b, 10);
        assert_eq!(divs.len(), 1);
        assert!(divs[0].right.contains("child 2"));
    }

    #[test]
    fn kind_mismatch_atom_vs_group() {
        let a = parse_sexpr("Nat");
        let b = parse_sexpr("(Nat)");
        let divs = structural_diff(&a, &b, 10);
        assert_eq!(divs.len(), 1);
        assert_eq!(divs[0].left, "Nat");
        assert_eq!(divs[0].right, "(Nat)");
    }

    #[test]
    fn max_divergences_respected() {
        // Many divergences: all 3 atoms differ
        let a = parse_sexpr("(A B C)");
        let b = parse_sexpr("(X Y Z)");
        let divs = structural_diff(&a, &b, 2);
        assert_eq!(divs.len(), 2); // only 2 reported, not 3
    }

    #[test]
    fn path_format() {
        let a = parse_sexpr("(Nat → (Bool × Nat))");
        let b = parse_sexpr("(Nat → (Bool × String))");
        let divs = structural_diff(&a, &b, 10);
        assert_eq!(divs.len(), 1);
        let path_str = format_path(&divs[0].path);
        // Path should show breadcrumbs through the nesting
        assert!(!path_str.is_empty());
        assert!(path_str.contains("→"));
    }

    #[test]
    fn bracket_vs_paren_mismatch() {
        let a = parse_sexpr("(x)");
        let b = parse_sexpr("[x]");
        let divs = structural_diff(&a, &b, 10);
        assert_eq!(divs.len(), 1);
    }

    #[test]
    fn term_structural_diff() {
        let a = parse_sexpr("(λx:Nat. (succ x))");
        let b = parse_sexpr("(λx:Nat. (pred x))");
        let divs = structural_diff(&a, &b, 10);
        assert_eq!(divs.len(), 1);
        assert_eq!(divs[0].left, "succ");
        assert_eq!(divs[0].right, "pred");
    }

    #[test]
    fn tyvar_vs_mu_var_divergence() {
        // The key ADR scenario: bare TyVar vs μ-variable
        let a = parse_sexpr("TyVar(\"TypeExpr\")");
        let b = parse_sexpr("TyVar(\"α_TypeExpr\")");
        let divs = structural_diff(&a, &b, 10);
        assert_eq!(divs.len(), 1);
        assert_eq!(divs[0].left, "\"TypeExpr\"");
        assert_eq!(divs[0].right, "\"α_TypeExpr\"");
    }

    #[test]
    fn format_path_empty() {
        assert_eq!(format_path(&[]), "root");
    }
}
