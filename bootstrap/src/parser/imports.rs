//! Parsing module and import declarations (`mod`, `use`).

use crate::ast::*;
use crate::error::ParseErrorKind;
use crate::span::Span;
use crate::token::TokenKind;

use super::Parser;

impl Parser<'_> {
    /// Parse a module declaration: `mod foo;` or `pub mod foo;`
    pub(super) fn parse_mod_decl(&mut self) -> Option<ModDecl> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        self.expect(TokenKind::Mod)?;

        // Check if the module name is a keyword (reserved or not)
        let current_kind = self.current().kind;
        if current_kind.is_keyword() || current_kind.is_reserved() {
            let kw_name = self.current_text().to_string();
            let span = self.current_span();
            let mut err = crate::error::ParseError::new(
                span,
                crate::error::ParseErrorKind::ReservedKeyword(kw_name.clone()),
            );
            err.expected = vec!["module name (identifier)".to_string()];
            self.errors.push(err);
            return None;
        } else if current_kind != TokenKind::Ident {
            self.error(ParseErrorKind::Expected("module name".to_string()));
            return None;
        }

        let name = self.parse_ident()?;
        let end = self.expect(TokenKind::Semi)?.end;

        Some(ModDecl {
            visibility,
            name,
            span: Span::new(start, end),
        })
    }

    /// Parse a use declaration: `use foo::bar;` or `pub use foo::{a, b};`
    pub(super) fn parse_use_decl(&mut self) -> Option<UseDecl> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        self.expect(TokenKind::Use)?;

        let tree = self.parse_use_tree()?;
        let end = self.expect(TokenKind::Semi)?.end;

        Some(UseDecl {
            visibility,
            tree,
            span: Span::new(start, end),
        })
    }

    /// Parse a use tree: `foo::bar`, `foo::{a, b}`, or `foo::*`
    fn parse_use_tree(&mut self) -> Option<UseTree> {
        // Parse the initial path, stopping before ::{ or ::*
        let path = self.parse_use_path()?;

        // Check for grouped import: `foo::{a, b}` or glob import: `foo::*`
        if self.check(TokenKind::ColonColon) {
            // Peek ahead to see if it's followed by `{` (group) or `*` (glob)
            if self.check_double_colon_brace() {
                self.advance(); // consume ::
                self.advance(); // consume {
                let group_start = self.prev_span().start;
                let mut items = Vec::new();

                if !self.check(TokenKind::RBrace) {
                    loop {
                        items.push(self.parse_use_tree()?);
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                        // Allow trailing comma
                        if self.check(TokenKind::RBrace) {
                            break;
                        }
                    }
                }

                let end = self.expect(TokenKind::RBrace)?.end;
                return Some(UseTree::Group {
                    prefix: path,
                    items,
                    span: Span::new(group_start, end),
                });
            } else if self.check_double_colon_star() {
                self.advance(); // consume ::
                let star_span = self.expect(TokenKind::Star)?;
                let span_start = path.span.start;
                return Some(UseTree::Glob {
                    prefix: path,
                    span: Span::new(span_start, star_span.end),
                });
            }
        }

        // Check for alias: `foo::bar as baz`
        if self.eat(TokenKind::As) {
            let alias = self.parse_ident()?;
            let span = Span::new(path.span.start, alias.span.end);
            return Some(UseTree::Alias { path, alias, span });
        }

        Some(UseTree::Path(path))
    }

    /// Parse a path for use statements, stopping before `::{}` or `::*`
    fn parse_use_path(&mut self) -> Option<Path> {
        let start = self.current_span().start;

        let first = self.parse_ident()?;
        let mut segments = vec![first];

        // Parse additional `::ident` segments, but stop before `::{` or `::*`
        while self.check(TokenKind::ColonColon)
            && !self.check_double_colon_brace()
            && !self.check_double_colon_star()
        {
            self.advance(); // consume ::
            let segment = self.parse_ident()?;
            segments.push(segment);
        }

        let end = segments.last().unwrap().span.end;
        Some(Path {
            segments,
            span: Span::new(start, end),
        })
    }

    /// Check if current position has `::` followed by `{`
    fn check_double_colon_brace(&self) -> bool {
        self.check(TokenKind::ColonColon) && self.check_ahead(TokenKind::LBrace)
    }

    /// Check if current position has `::` followed by `*`
    fn check_double_colon_star(&self) -> bool {
        self.check(TokenKind::ColonColon) && self.check_ahead(TokenKind::Star)
    }
}
