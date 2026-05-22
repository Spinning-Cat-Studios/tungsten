//! Lightweight parser for Core IR type display format.
//!
//! Parses the canonical output of `Type::Display` into a simple AST
//! for step-by-step explanation. This only handles the display format,
//! NOT full Tungsten source syntax.
//!
//! Grammar:
//!   τ  ::=  Nat | Bool | Unit | Void | String | Prop
//!        |  τ → τ           (arrow / function)
//!        |  τ × τ           (product)
//!        |  τ + τ           (sum)
//!        |  μα. τ           (recursive / mu)
//!        |  ∀α. τ           (universal / forall)
//!        |  α               (type variable)
//!        |  ( τ )           (grouping)

use std::fmt;

/// Parsed type AST for explanation.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeAst {
    /// Base types: Nat, Bool, Unit, Void, String, Prop
    Base(String),
    /// Type variable
    TyVar(String),
    /// Arrow type: τ → τ
    Arrow(Box<TypeAst>, Box<TypeAst>),
    /// Product type: τ × τ
    Product(Box<TypeAst>, Box<TypeAst>),
    /// Sum type: τ + τ
    Sum(Box<TypeAst>, Box<TypeAst>),
    /// Recursive type: μα. τ
    Mu(String, Box<TypeAst>),
    /// Universal type: ∀α. τ
    Forall(String, Box<TypeAst>),
    /// Type error placeholder
    Error,
}

impl fmt::Display for TypeAst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeAst::Base(name) => write!(f, "{name}"),
            TypeAst::TyVar(name) => write!(f, "{name}"),
            TypeAst::Arrow(a, b) => write!(f, "({a} → {b})"),
            TypeAst::Product(a, b) => write!(f, "({a} × {b})"),
            TypeAst::Sum(a, b) => write!(f, "({a} + {b})"),
            TypeAst::Mu(v, body) => write!(f, "μ{v}. {body}"),
            TypeAst::Forall(v, body) => write!(f, "∀{v}. {body}"),
            TypeAst::Error => write!(f, "<type error>"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tokenizer
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    /// μ
    Mu,
    /// ∀
    Forall,
    /// →
    Arrow,
    /// ×
    Product,
    /// +
    Sum,
    /// .
    Dot,
    /// (
    LParen,
    /// )
    RParen,
    /// Identifier (type name or type variable)
    Ident(String),
    /// <type error>
    Error,
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            'μ' => {
                chars.next();
                tokens.push(Token::Mu);
            }
            '∀' => {
                chars.next();
                tokens.push(Token::Forall);
            }
            '→' => {
                chars.next();
                tokens.push(Token::Arrow);
            }
            '×' => {
                chars.next();
                tokens.push(Token::Product);
            }
            '+' => {
                chars.next();
                tokens.push(Token::Sum);
            }
            '.' => {
                chars.next();
                tokens.push(Token::Dot);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            '<' => {
                // Handle <type error>
                let rest: String = chars.clone().collect();
                if rest.starts_with("<type error>") {
                    for _ in 0.."<type error>".len() {
                        chars.next();
                    }
                    tokens.push(Token::Error);
                } else {
                    return Err(format!("unexpected character: `{ch}`"));
                }
            }
            _ if ch.is_alphanumeric() || ch == '_' || ch == 'α' || ch == '@' => {
                let mut ident = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' || c == 'α' || c == '@' {
                        ident.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Ident(ident));
            }
            _ => {
                return Err(format!("unexpected character: `{ch}`"));
            }
        }
    }

    Ok(tokens)
}

// ─────────────────────────────────────────────────────────────────────────────
// Parser
// ─────────────────────────────────────────────────────────────────────────────

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos)?.clone();
        self.pos += 1;
        Some(tok)
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        match self.next() {
            Some(ref tok) if tok == expected => Ok(()),
            Some(tok) => Err(format!("expected {expected:?}, found {tok:?}")),
            None => Err(format!("expected {expected:?}, found end of input")),
        }
    }

    /// Parse a complete type expression.
    fn parse_type(&mut self) -> Result<TypeAst, String> {
        self.parse_binding()
    }

    /// Parse binding forms: μα. τ  |  ∀α. τ  |  binary
    fn parse_binding(&mut self) -> Result<TypeAst, String> {
        match self.peek() {
            Some(Token::Mu) => {
                self.next(); // consume μ
                let var = self.parse_ident()?;
                self.expect(&Token::Dot)?;
                let body = self.parse_binding()?;
                Ok(TypeAst::Mu(var, Box::new(body)))
            }
            Some(Token::Forall) => {
                self.next(); // consume ∀
                let var = self.parse_ident()?;
                self.expect(&Token::Dot)?;
                let body = self.parse_binding()?;
                Ok(TypeAst::Forall(var, Box::new(body)))
            }
            _ => self.parse_sum(),
        }
    }

    /// Parse sum: τ + τ  (left-associative, lowest precedence among binary ops)
    fn parse_sum(&mut self) -> Result<TypeAst, String> {
        let mut lhs = self.parse_product()?;
        while matches!(self.peek(), Some(Token::Sum)) {
            self.next();
            let rhs = self.parse_product()?;
            lhs = TypeAst::Sum(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// Parse product: τ × τ  (left-associative)
    fn parse_product(&mut self) -> Result<TypeAst, String> {
        let mut lhs = self.parse_arrow()?;
        while matches!(self.peek(), Some(Token::Product)) {
            self.next();
            let rhs = self.parse_arrow()?;
            lhs = TypeAst::Product(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// Parse arrow: τ → τ  (right-associative)
    fn parse_arrow(&mut self) -> Result<TypeAst, String> {
        let lhs = self.parse_atom()?;
        if matches!(self.peek(), Some(Token::Arrow)) {
            self.next();
            let rhs = self.parse_arrow()?;
            Ok(TypeAst::Arrow(Box::new(lhs), Box::new(rhs)))
        } else {
            Ok(lhs)
        }
    }

    /// Parse atomic types: base types, variables, parenthesized expressions
    fn parse_atom(&mut self) -> Result<TypeAst, String> {
        match self.peek() {
            Some(Token::LParen) => {
                self.next(); // consume (
                let inner = self.parse_type()?;
                self.expect(&Token::RParen)?;
                Ok(inner)
            }
            Some(Token::Error) => {
                self.next();
                Ok(TypeAst::Error)
            }
            Some(Token::Ident(_)) => {
                let name = self.parse_ident()?;
                Ok(ident_to_type_ast(name))
            }
            Some(tok) => Err(format!("unexpected token: {tok:?}")),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn parse_ident(&mut self) -> Result<String, String> {
        match self.next() {
            Some(Token::Ident(name)) => Ok(name),
            Some(tok) => Err(format!("expected identifier, found {tok:?}")),
            None => Err("expected identifier, found end of input".to_string()),
        }
    }
}

/// Classify an identifier as a base type or type variable.
fn ident_to_type_ast(name: String) -> TypeAst {
    match name.as_str() {
        "Nat" | "Bool" | "Unit" | "Void" | "String" | "Prop" => TypeAst::Base(name),
        _ => TypeAst::TyVar(name),
    }
}

/// Parse a type string into a `TypeAst`.
pub fn parse_type(input: &str) -> Result<TypeAst, String> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Err("empty type string".to_string());
    }
    let mut parser = Parser::new(tokens);
    let ast = parser.parse_type()?;

    // Ensure all tokens were consumed
    if parser.pos < parser.tokens.len() {
        return Err(format!(
            "unexpected trailing input at position {}",
            parser.pos
        ));
    }

    Ok(ast)
}

// Tests: type_parser_tests.rs
#[cfg(test)]
#[path = "type_parser_tests.rs"]
mod tests;
