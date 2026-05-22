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

mod comments_and_combined;
mod literals_and_escapes;
mod tokens_and_keywords;
