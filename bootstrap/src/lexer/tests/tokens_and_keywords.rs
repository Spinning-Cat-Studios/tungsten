use super::*;

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
