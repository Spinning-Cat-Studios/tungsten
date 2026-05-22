//! S-expression parsing and display tests for `tungsten diff-core`.

#[cfg(test)]
mod sexpr_tests {
    use crate::diff_core::sexpr::{parse_sexpr, SExpr};

    #[test]
    fn parse_bare_atom() {
        let result = parse_sexpr("Nat");
        assert_eq!(result, vec![SExpr::Atom("Nat".to_string())]);
    }

    #[test]
    fn parse_paren_expr() {
        let result = parse_sexpr("(Nat → Nat)");
        assert_eq!(
            result,
            vec![SExpr::Paren(vec![
                SExpr::Atom("Nat".to_string()),
                SExpr::Atom("→".to_string()),
                SExpr::Atom("Nat".to_string()),
            ])]
        );
    }

    #[test]
    fn parse_nested_parens() {
        let result = parse_sexpr("(f (g x))");
        assert_eq!(
            result,
            vec![SExpr::Paren(vec![
                SExpr::Atom("f".to_string()),
                SExpr::Paren(vec![
                    SExpr::Atom("g".to_string()),
                    SExpr::Atom("x".to_string()),
                ]),
            ])]
        );
    }

    #[test]
    fn parse_brackets() {
        let result = parse_sexpr("[Nat]");
        assert_eq!(
            result,
            vec![SExpr::Bracket(vec![SExpr::Atom("Nat".to_string())])]
        );
    }

    #[test]
    fn parse_mixed_parens_brackets() {
        let result = parse_sexpr("(fold [μα. T] x)");
        assert_eq!(
            result,
            vec![SExpr::Paren(vec![
                SExpr::Atom("fold".to_string()),
                SExpr::Bracket(vec![
                    SExpr::Atom("μα.".to_string()),
                    SExpr::Atom("T".to_string()),
                ]),
                SExpr::Atom("x".to_string()),
            ])]
        );
    }

    #[test]
    fn parse_mu_type() {
        let result = parse_sexpr("μα_List. (Unit + α_List)");
        assert_eq!(
            result,
            vec![
                SExpr::Atom("μα_List.".to_string()),
                SExpr::Paren(vec![
                    SExpr::Atom("Unit".to_string()),
                    SExpr::Atom("+".to_string()),
                    SExpr::Atom("α_List".to_string()),
                ]),
            ]
        );
    }

    #[test]
    fn parse_string_literal() {
        let result = parse_sexpr("\"hello world\"");
        assert_eq!(result, vec![SExpr::Atom("\"hello world\"".to_string())]);
    }

    #[test]
    fn parse_escaped_string_literal() {
        let result = parse_sexpr("\"say \\\"hi\\\"\"");
        assert_eq!(result, vec![SExpr::Atom("\"say \\\"hi\\\"\"".to_string())]);
    }

    #[test]
    fn parse_empty() {
        let result = parse_sexpr("");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_whitespace_only() {
        let result = parse_sexpr("   \t\n  ");
        assert!(result.is_empty());
    }

    #[test]
    fn display_roundtrip_atom() {
        let expr = SExpr::Atom("Nat".to_string());
        assert_eq!(expr.display(), "Nat");
    }

    #[test]
    fn display_roundtrip_paren() {
        let expr = SExpr::Paren(vec![
            SExpr::Atom("Nat".to_string()),
            SExpr::Atom("→".to_string()),
            SExpr::Atom("Nat".to_string()),
        ]);
        assert_eq!(expr.display(), "(Nat → Nat)");
    }

    #[test]
    fn display_roundtrip_nested() {
        let input = "(f (g x))";
        let parsed = parse_sexpr(input);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].display(), input);
    }

    #[test]
    fn parse_lambda_term() {
        let result = parse_sexpr("(λx:Nat. (succ x))");
        assert_eq!(
            result,
            vec![SExpr::Paren(vec![
                SExpr::Atom("λx:Nat.".to_string()),
                SExpr::Paren(vec![
                    SExpr::Atom("succ".to_string()),
                    SExpr::Atom("x".to_string()),
                ]),
            ])]
        );
    }

    #[test]
    fn parse_adt_type_with_pipes() {
        // ADT display format: Name[Ctor1(payload) | Ctor2]
        let result = parse_sexpr("Color[Red | Green | Blue]");
        assert_eq!(
            result,
            vec![
                SExpr::Atom("Color".to_string()),
                SExpr::Bracket(vec![
                    SExpr::Atom("Red".to_string()),
                    SExpr::Atom("|".to_string()),
                    SExpr::Atom("Green".to_string()),
                    SExpr::Atom("|".to_string()),
                    SExpr::Atom("Blue".to_string()),
                ]),
            ]
        );
    }
}
