use super::*;

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
fn test_hex_escape_in_string() {
    let (tokens, errors) = Lexer::new(r#""\x1b""#).tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::StringLiteral);

    let (tokens, errors) = Lexer::new(r#""\x00\xFF""#).tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::StringLiteral);
}

#[test]
fn test_hex_escape_in_char() {
    let (tokens, errors) = Lexer::new(r"'\x1b'").tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::CharLiteral);
}

#[test]
fn test_invalid_hex_escape() {
    let (_, errors) = Lexer::new(r#""\x1""#).tokenize();
    assert!(
        !errors.is_empty(),
        "Expected error for incomplete hex escape"
    );
    assert!(matches!(errors[0].kind, LexErrorKind::InvalidHexEscape));

    let (_, errors) = Lexer::new(r#""\xGG""#).tokenize();
    assert!(!errors.is_empty(), "Expected error for invalid hex digit");
    assert!(matches!(errors[0].kind, LexErrorKind::InvalidHexEscape));
}

#[test]
fn test_unicode_escape_in_string() {
    let (tokens, errors) = Lexer::new(r#""\u{1F600}""#).tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::StringLiteral);

    let (tokens, errors) = Lexer::new(r#""\u{A}""#).tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::StringLiteral);
}

#[test]
fn test_unicode_escape_in_char() {
    let (tokens, errors) = Lexer::new(r"'\u{41}'").tokenize();
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    assert_eq!(tokens[0].kind, TokenKind::CharLiteral);
}

#[test]
fn test_invalid_unicode_escape() {
    let (_, errors) = Lexer::new(r#""\u1234""#).tokenize();
    assert!(!errors.is_empty(), "Expected error for missing brace");
    assert!(matches!(errors[0].kind, LexErrorKind::InvalidUnicodeEscape));

    let (_, errors) = Lexer::new(r#""\u{}""#).tokenize();
    assert!(
        !errors.is_empty(),
        "Expected error for empty unicode escape"
    );
    assert!(matches!(errors[0].kind, LexErrorKind::InvalidUnicodeEscape));
}

#[test]
fn test_single_char_token_known_chars() {
    use crate::lexer::single_char_token;
    assert_eq!(single_char_token('('), Some(TokenKind::LParen));
    assert_eq!(single_char_token(')'), Some(TokenKind::RParen));
    assert_eq!(single_char_token('{'), Some(TokenKind::LBrace));
    assert_eq!(single_char_token('}'), Some(TokenKind::RBrace));
    assert_eq!(single_char_token(','), Some(TokenKind::Comma));
    assert_eq!(single_char_token(';'), Some(TokenKind::Semi));
    assert_eq!(single_char_token('@'), Some(TokenKind::At));
    assert_eq!(single_char_token('%'), Some(TokenKind::Percent));
}

#[test]
fn test_single_char_token_non_punctuation() {
    use crate::lexer::single_char_token;
    assert_eq!(single_char_token('a'), None);
    assert_eq!(single_char_token('0'), None);
    assert_eq!(single_char_token('+'), None);
    assert_eq!(single_char_token('-'), None);
    assert_eq!(single_char_token('/'), None);
}
