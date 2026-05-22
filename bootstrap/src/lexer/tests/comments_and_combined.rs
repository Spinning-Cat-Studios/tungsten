use super::*;

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
