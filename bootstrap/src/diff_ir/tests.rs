#[cfg(test)]
mod parser_tests {
    use crate::diff_ir::parser::parse_ir_defs;

    #[test]
    fn parse_type_definition() {
        let ir = "%MyStruct = type { i32, [8 x i8] }\n";
        let defs = parse_ir_defs(ir);
        assert_eq!(defs.types.len(), 1);
        assert!(defs.types.contains_key("MyStruct"));
    }

    #[test]
    fn parse_function_definition() {
        let ir = "define i32 @main() {\nentry:\n  ret i32 0\n}\n";
        let defs = parse_ir_defs(ir);
        assert_eq!(defs.functions.len(), 1);
        assert!(defs.functions.contains_key("main"));
    }

    #[test]
    fn parse_declare() {
        let ir = "declare i32 @printf(ptr, ...)\n";
        let defs = parse_ir_defs(ir);
        assert_eq!(defs.functions.len(), 1);
        assert!(defs.functions.contains_key("printf"));
    }

    #[test]
    fn empty_ir_produces_empty_defs() {
        let defs = parse_ir_defs("");
        assert!(defs.types.is_empty());
        assert!(defs.functions.is_empty());
    }
}
