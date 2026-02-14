//! Lexer tests.

use super::*;

fn lex(source: &str) -> Vec<Token> {
    let lexer = Lexer::new(source);
    let (tokens, _) = lexer.tokenize();
    tokens
}

fn lex_kinds(source: &str) -> Vec<TokenKind> {
    lex(source).into_iter().map(|t| t.kind).collect()
}

fn lex_with_errors(source: &str) -> (Vec<TokenKind>, Vec<LexError>) {
    let lexer = Lexer::new(source);
    let (tokens, errors) = lexer.tokenize();
    (tokens.into_iter().map(|t| t.kind).collect(), errors)
}

#[test]
fn test_empty() {
    assert_eq!(lex_kinds(""), vec![TokenKind::Eof]);
}

#[test]
fn test_whitespace_filtered() {
    assert_eq!(lex_kinds("   "), vec![TokenKind::Eof]);
}

#[test]
fn test_identifiers() {
    assert_eq!(
        lex_kinds("foo bar _baz FooBar foo123"),
        vec![
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_keywords() {
    assert_eq!(
        lex_kinds("fn let if else match return type struct enum"),
        vec![
            TokenKind::Fn,
            TokenKind::Let,
            TokenKind::If,
            TokenKind::Else,
            TokenKind::Match,
            TokenKind::Return,
            TokenKind::Type,
            TokenKind::Struct,
            TokenKind::Enum,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_proof_keywords() {
    assert_eq!(
        lex_kinds("theorem lemma axiom by have show assume forall exists Prop sorry"),
        vec![
            TokenKind::Theorem,
            TokenKind::Lemma,
            TokenKind::Axiom,
            TokenKind::By,
            TokenKind::Have,
            TokenKind::Show,
            TokenKind::Assume,
            TokenKind::Forall,
            TokenKind::Exists,
            TokenKind::Prop,
            TokenKind::Sorry,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_type_keywords() {
    assert_eq!(
        lex_kinds("Bool Nat Unit Void"),
        vec![
            TokenKind::Bool,
            TokenKind::Nat,
            TokenKind::Unit,
            TokenKind::Void,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_reserved_keywords_error() {
    let (kinds, errors) = lex_with_errors("async await");
    assert_eq!(
        kinds,
        vec![TokenKind::Async, TokenKind::Await, TokenKind::Eof]
    );
    assert_eq!(errors.len(), 2);
    assert!(matches!(errors[0].kind, LexErrorKind::ReservedKeyword(_)));
}

#[test]
fn test_numbers() {
    assert_eq!(
        lex_kinds("0 42 1_000_000"),
        vec![
            TokenKind::IntLiteral,
            TokenKind::IntLiteral,
            TokenKind::IntLiteral,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_hex_octal_binary() {
    assert_eq!(
        lex_kinds("0x2A 0o52 0b101010"),
        vec![
            TokenKind::IntLiteral,
            TokenKind::IntLiteral,
            TokenKind::IntLiteral,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_invalid_number() {
    let (kinds, errors) = lex_with_errors("0x 0b");
    assert_eq!(
        kinds,
        vec![TokenKind::Error, TokenKind::Error, TokenKind::Eof]
    );
    assert_eq!(errors.len(), 2);
}

#[test]
fn test_strings() {
    let tokens = lex(r#""hello" "world""#);
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].kind, TokenKind::StringLiteral);
    assert_eq!(tokens[1].kind, TokenKind::StringLiteral);
}

#[test]
fn test_string_escapes() {
    let (kinds, errors) = lex_with_errors(r#""hello\nworld""#);
    assert_eq!(kinds, vec![TokenKind::StringLiteral, TokenKind::Eof]);
    assert!(errors.is_empty());
}

#[test]
fn test_unterminated_string() {
    let (kinds, errors) = lex_with_errors(r#""hello"#);
    assert_eq!(kinds, vec![TokenKind::Error, TokenKind::Eof]);
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0].kind, LexErrorKind::UnterminatedString));
}

#[test]
fn test_char_literals() {
    assert_eq!(
        lex_kinds("'a' 'b' '\\n'"),
        vec![
            TokenKind::CharLiteral,
            TokenKind::CharLiteral,
            TokenKind::CharLiteral,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_empty_char_literal() {
    let (kinds, errors) = lex_with_errors("''");
    assert_eq!(kinds, vec![TokenKind::Error, TokenKind::Eof]);
    assert!(matches!(errors[0].kind, LexErrorKind::EmptyCharLiteral));
}

#[test]
fn test_delimiters() {
    assert_eq!(
        lex_kinds("(){}[]<>"),
        vec![
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_punctuation() {
    assert_eq!(
        lex_kinds(", ; : :: . .. ... => -> @ # ?"),
        vec![
            TokenKind::Comma,
            TokenKind::Semi,
            TokenKind::Colon,
            TokenKind::ColonColon,
            TokenKind::Dot,
            TokenKind::DotDot,
            TokenKind::DotDotDot,
            TokenKind::FatArrow,
            TokenKind::Arrow,
            TokenKind::At,
            TokenKind::Hash,
            TokenKind::Question,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_operators() {
    assert_eq!(
        lex_kinds("= == != < <= > >= + - * / % & && | || ! ^ ~"),
        vec![
            TokenKind::Eq,
            TokenKind::EqEq,
            TokenKind::Ne,
            TokenKind::Lt,
            TokenKind::Le,
            TokenKind::Gt,
            TokenKind::Ge,
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::Amp,
            TokenKind::AmpAmp,
            TokenKind::Pipe,
            TokenKind::PipePipe,
            TokenKind::Bang,
            TokenKind::Caret,
            TokenKind::Tilde,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_line_comment() {
    let lexer = Lexer::new("foo // comment\nbar");
    let (tokens, _) = lexer.tokenize_all();
    let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
    assert!(kinds.contains(&TokenKind::LineComment));
}

#[test]
fn test_block_comment() {
    let lexer = Lexer::new("foo /* comment */ bar");
    let (tokens, _) = lexer.tokenize_all();
    let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
    assert!(kinds.contains(&TokenKind::BlockComment));
}

#[test]
fn test_nested_block_comment() {
    let (kinds, errors) = lex_with_errors("foo /* outer /* inner */ outer */ bar");
    assert!(errors.is_empty());
    assert_eq!(
        kinds,
        vec![TokenKind::Ident, TokenKind::Ident, TokenKind::Eof]
    );
}

#[test]
fn test_unterminated_block_comment() {
    let (_, errors) = lex_with_errors("/* unterminated");
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        errors[0].kind,
        LexErrorKind::UnterminatedBlockComment
    ));
}

#[test]
fn test_doc_comment() {
    let lexer = Lexer::new("/// doc comment\n//! inner doc");
    let (tokens, _) = lexer.tokenize_all();
    let doc_count = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::DocComment)
        .count();
    assert_eq!(doc_count, 2);
}

#[test]
fn test_function_signature() {
    assert_eq!(
        lex_kinds("fn add(x: Nat, y: Nat) -> Nat"),
        vec![
            TokenKind::Fn,
            TokenKind::Ident,
            TokenKind::LParen,
            TokenKind::Ident,
            TokenKind::Colon,
            TokenKind::Nat,
            TokenKind::Comma,
            TokenKind::Ident,
            TokenKind::Colon,
            TokenKind::Nat,
            TokenKind::RParen,
            TokenKind::Arrow,
            TokenKind::Nat,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_theorem_signature() {
    assert_eq!(
        lex_kinds("theorem add_comm(x: Nat, y: Nat): x + y == y + x"),
        vec![
            TokenKind::Theorem,
            TokenKind::Ident,
            TokenKind::LParen,
            TokenKind::Ident,
            TokenKind::Colon,
            TokenKind::Nat,
            TokenKind::Comma,
            TokenKind::Ident,
            TokenKind::Colon,
            TokenKind::Nat,
            TokenKind::RParen,
            TokenKind::Colon,
            TokenKind::Ident,
            TokenKind::Plus,
            TokenKind::Ident,
            TokenKind::EqEq,
            TokenKind::Ident,
            TokenKind::Plus,
            TokenKind::Ident,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_match_expression() {
    assert_eq!(
        lex_kinds("match x { 0 => true, _ => false }"),
        vec![
            TokenKind::Match,
            TokenKind::Ident,
            TokenKind::LBrace,
            TokenKind::IntLiteral,
            TokenKind::FatArrow,
            TokenKind::True,
            TokenKind::Comma,
            TokenKind::Underscore,
            TokenKind::FatArrow,
            TokenKind::False,
            TokenKind::RBrace,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_token_spans() {
    let source = "fn foo";
    let tokens = lex(source);

    assert_eq!(tokens[0].kind, TokenKind::Fn);
    assert_eq!(tokens[0].span, Span::new(0, 2));
    assert_eq!(tokens[0].text(source), "fn");

    assert_eq!(tokens[1].kind, TokenKind::Ident);
    assert_eq!(tokens[1].span, Span::new(3, 6));
    assert_eq!(tokens[1].text(source), "foo");
}

#[test]
fn test_underscore() {
    assert_eq!(
        lex_kinds("_ _foo"),
        vec![TokenKind::Underscore, TokenKind::Ident, TokenKind::Eof]
    );
}

#[test]
fn test_booleans() {
    assert_eq!(
        lex_kinds("true false"),
        vec![TokenKind::True, TokenKind::False, TokenKind::Eof]
    );
}

#[test]
fn test_hex_escape_in_string() {
    // Valid hex escapes
    let (tokens, errors) = Lexer::new(r#""\x1b""#).tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::StringLiteral);

    let (tokens, errors) = Lexer::new(r#""\x00\xFF""#).tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::StringLiteral);
}

#[test]
fn test_hex_escape_in_char() {
    // Valid hex escape in char literal
    let (tokens, errors) = Lexer::new(r"'\x1b'").tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::CharLiteral);
}

#[test]
fn test_invalid_hex_escape() {
    // Invalid: not enough digits
    let (_, errors) = Lexer::new(r#""\x1""#).tokenize();
    assert!(
        !errors.is_empty(),
        "Expected error for incomplete hex escape"
    );
    assert!(matches!(errors[0].kind, LexErrorKind::InvalidHexEscape));

    // Invalid: non-hex character
    let (_, errors) = Lexer::new(r#""\xGG""#).tokenize();
    assert!(!errors.is_empty(), "Expected error for invalid hex digit");
    assert!(matches!(errors[0].kind, LexErrorKind::InvalidHexEscape));
}

#[test]
fn test_unicode_escape_in_string() {
    // Valid unicode escapes
    let (tokens, errors) = Lexer::new(r#""\u{1F600}""#).tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::StringLiteral);

    // Single digit
    let (tokens, errors) = Lexer::new(r#""\u{A}""#).tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::StringLiteral);
}

#[test]
fn test_unicode_escape_in_char() {
    // Valid unicode escape in char literal
    let (tokens, errors) = Lexer::new(r"'\u{41}'").tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::CharLiteral);
}

#[test]
fn test_invalid_unicode_escape() {
    // Invalid: missing opening brace
    let (_, errors) = Lexer::new(r#""\u1234""#).tokenize();
    assert!(!errors.is_empty(), "Expected error for missing brace");
    assert!(matches!(errors[0].kind, LexErrorKind::InvalidUnicodeEscape));

    // Invalid: empty braces
    let (_, errors) = Lexer::new(r#""\u{}""#).tokenize();
    assert!(
        !errors.is_empty(),
        "Expected error for empty unicode escape"
    );
    assert!(matches!(errors[0].kind, LexErrorKind::InvalidUnicodeEscape));
}
