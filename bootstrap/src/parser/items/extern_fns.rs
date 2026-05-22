//! Parsing extern function declarations.

use crate::ast::*;
use crate::error::ParseErrorKind;
use crate::span::{Span, Spanned};
use crate::token::TokenKind;

use crate::parser::Parser;
impl Parser<'_> {
    /// Parse an extern function declaration: `extern fn name(params) -> RetType`
    /// Or with ABI: `extern "C" fn name(params) -> RetType`
    pub(super) fn parse_extern_fn(&mut self) -> Option<ExternFnDef> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        self.expect(TokenKind::Extern)?;

        // Optional ABI string
        let abi = if self.check(TokenKind::StringLiteral) {
            let s = self.get_string_literal()?;
            self.advance();
            s
        } else {
            "C".to_string() // Default to C ABI
        };

        // Check for common mistake: `extern "name" : Type` instead of `extern "C" fn name(...) -> Type`
        if self.check(TokenKind::Colon) {
            self.report_extern_missing_fn(start);
            return None;
        }

        if !self.eat(TokenKind::Fn) {
            self.error(ParseErrorKind::Expected(
                "`fn` keyword after `extern` or ABI string".to_string(),
            ));
            return None;
        }

        let name = self.parse_ident()?;
        let params = self.parse_extern_params()?;

        // Return type
        self.expect(TokenKind::Arrow)?;
        let return_type = self.parse_type()?;
        let end = return_type.span().end;

        Some(ExternFnDef {
            visibility,
            name,
            symbol: None, // TODO: support explicit symbol name
            abi,
            params,
            return_type,
            span: Span::new(start, end),
        })
    }

    /// Report error when extern declaration uses `: Type` syntax instead of function syntax.
    fn report_extern_missing_fn(&mut self, start: u32) {
        let span = self.current_span();
        self.errors.push(
            crate::error::ParseError::new(
                span,
                crate::error::ParseErrorKind::Expected("`fn` keyword".to_string()),
            )
            .with_expected(vec!["`fn`".to_string()]),
        );
        self.errors.last_mut().map(|e| {
            e.suggestions.push(crate::error::Suggestion::new(
                Span::new(start, span.end),
                "extern \"C\" fn name(arg: Type) -> RetType",
                "extern declarations use function syntax: `extern \"C\" fn name(params) -> ReturnType`",
            ));
        });
    }

    /// Parse extern function parameters: `(name: Type, ...)`
    fn parse_extern_params(&mut self) -> Option<Vec<ExternParam>> {
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        while !self.check(TokenKind::RParen) && !self.at_eof() {
            let param_start = self.current_span().start;
            let param_name = self.parse_ident()?;
            self.expect(TokenKind::Colon)?;
            let param_ty = self.parse_type()?;
            let param_end = param_ty.span().end;
            params.push(ExternParam {
                name: param_name,
                ty: param_ty,
                span: Span::new(param_start, param_end),
            });

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;
        Some(params)
    }
}
