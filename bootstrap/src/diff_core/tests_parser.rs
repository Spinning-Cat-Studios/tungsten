//! Parser tests for `tungsten diff-core`.

#[cfg(test)]
mod parser_tests {
    use crate::diff_core::parser::parse_core_defs;

    #[test]
    fn parse_single_def() {
        let input = r#"
┌─────────────────────────────────────────────────────────────┐
│  Definition: main                                          │
│  Type: (Nat → Nat)                                         │
│                                                            │
│  Term: (λx:Nat. (succ x))                                 │
│  Free TyVars: ∅                                            │
└─────────────────────────────────────────────────────────────┘
"#;
        let defs = parse_core_defs(input);
        assert_eq!(defs.defs.len(), 1);
        let main = &defs.defs["main"];
        assert_eq!(main.name, "main");
        assert_eq!(main.ty, "(Nat → Nat)");
        assert!(main.term.contains("λx:Nat"));
    }

    #[test]
    fn parse_multiple_defs() {
        let input = r#"
┌─────────────────────────────────────────────────────────────┐
│  Definition: foo                                           │
│  Type: Nat                                                 │
│                                                            │
│  Term: zero                                                │
│  Free TyVars: ∅                                            │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  Definition: bar                                           │
│  Type: Bool                                                │
│                                                            │
│  Term: true                                                │
│  Free TyVars: ∅                                            │
└─────────────────────────────────────────────────────────────┘
"#;
        let defs = parse_core_defs(input);
        assert_eq!(defs.defs.len(), 2);
        assert!(defs.defs.contains_key("foo"));
        assert!(defs.defs.contains_key("bar"));
    }

    #[test]
    fn parse_empty_input() {
        let defs = parse_core_defs("");
        assert!(defs.defs.is_empty());
    }
}
