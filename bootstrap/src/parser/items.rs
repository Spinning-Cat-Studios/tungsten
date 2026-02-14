//! Parsing top-level items: functions, types, theorems, axioms.

use crate::ast::*;
use crate::error::ParseErrorKind;
use crate::span::{Span, Spanned};
use crate::token::TokenKind;

use super::Parser;

impl Parser<'_> {
    /// Parse a visibility modifier: `pub`, `pub(crate)`, or nothing (private).
    fn parse_visibility(&mut self) -> Visibility {
        if self.eat(TokenKind::Pub) {
            if self.check(TokenKind::LParen) {
                self.advance(); // consume (
                if self.eat(TokenKind::Crate) {
                    if !self.eat(TokenKind::RParen) {
                        self.error(ParseErrorKind::Expected(
                            "`)` after `pub(crate`".to_string(),
                        ));
                    }
                    Visibility::Crate
                } else {
                    self.error(ParseErrorKind::Expected("`crate` after `pub(`".to_string()));
                    // Try to recover by consuming until )
                    while !self.check(TokenKind::RParen) && !self.at_eof() {
                        self.advance();
                    }
                    self.eat(TokenKind::RParen);
                    Visibility::Private // Error recovery: treat as private
                }
            } else {
                Visibility::Public
            }
        } else {
            Visibility::Private
        }
    }

    pub(super) fn parse_item(&mut self) -> Option<Item> {
        // Check for stray semicolons (common mistake from other languages)
        if self.check(TokenKind::Semi) {
            let span = self.current_span();
            self.errors.push(crate::error::ParseError::new(
                span,
                crate::error::ParseErrorKind::UnexpectedToken(";".to_string()),
            ));
            self.errors.last_mut().map(|e| {
                e.suggestions.push(crate::error::Suggestion::new(
                    span,
                    "",
                    "Tungsten declarations don't require trailing semicolons; remove the `;`",
                ));
            });
            self.advance(); // Skip the semicolon
                            // Try to continue parsing
            if !self.at_eof() {
                return self.parse_item();
            }
            return None;
        }

        // Check for visibility prefix first
        let has_pub = self.check(TokenKind::Pub);
        let has_pub_crate = has_pub && self.check_ahead(TokenKind::LParen);

        // Parse visibility if present, but don't consume it for mod/use (they handle it themselves)
        // For other items, we need to peek ahead to determine what kind of item it is

        if has_pub || has_pub_crate {
            // Peek past the visibility to see what item follows
            let item_kind = if has_pub_crate {
                // pub(crate) - need to look further ahead
                self.peek_past_pub_crate()
            } else {
                // pub - look one ahead
                self.peek_ahead().kind
            };

            match item_kind {
                TokenKind::Mod => Some(Item::Mod(self.parse_mod_decl()?)),
                TokenKind::Use => Some(Item::Use(self.parse_use_decl()?)),
                TokenKind::Fn => Some(Item::Function(self.parse_function()?)),
                TokenKind::Type => self.parse_type_item(),
                TokenKind::Struct => Some(Item::TypeDef(self.parse_struct()?)),
                TokenKind::Enum => Some(Item::TypeDef(self.parse_enum()?)),
                TokenKind::Theorem => Some(Item::Theorem(self.parse_theorem()?)),
                TokenKind::Lemma => Some(Item::Lemma(self.parse_theorem()?)),
                TokenKind::Axiom => Some(Item::Axiom(self.parse_axiom()?)),
                TokenKind::Extern => Some(Item::ExternFn(self.parse_extern_fn()?)),
                TokenKind::Let => {
                    // Common mistake: `pub let` is not valid
                    let span = self.current_span();
                    let pub_span = if has_pub_crate {
                        Span::new(self.peek_n(0).span.start, self.peek_n(3).span.end)
                    } else {
                        self.current_span()
                    };
                    self.errors.push(crate::error::ParseError::new(
                        span,
                        crate::error::ParseErrorKind::Expected(
                            "item that can have visibility".to_string(),
                        ),
                    ));
                    self.errors.last_mut().map(|e| {
                        e.suggestions.push(crate::error::Suggestion::new(
                            pub_span,
                            "",
                            "visibility modifiers can only be applied to: fn, type, struct, enum, mod, use, theorem, lemma, axiom, extern",
                        ));
                    });
                    None
                }
                _ => {
                    let span = self.current_span();
                    self.errors.push(crate::error::ParseError::new(
                        span,
                        crate::error::ParseErrorKind::Expected(
                            "item after visibility modifier".to_string(),
                        ),
                    ));
                    self.errors.last_mut().map(|e| {
                        e.suggestions.push(crate::error::Suggestion::new(
                            span,
                            "",
                            "visibility modifiers can only be applied to: fn, type, struct, enum, mod, use, theorem, lemma, axiom, extern",
                        ));
                    });
                    None
                }
            }
        } else {
            match self.current().kind {
                TokenKind::Fn => Some(Item::Function(self.parse_function()?)),
                TokenKind::Type => self.parse_type_item(),
                TokenKind::Struct => Some(Item::TypeDef(self.parse_struct()?)),
                TokenKind::Enum => Some(Item::TypeDef(self.parse_enum()?)),
                TokenKind::Theorem => Some(Item::Theorem(self.parse_theorem()?)),
                TokenKind::Lemma => Some(Item::Lemma(self.parse_theorem()?)),
                TokenKind::Axiom => Some(Item::Axiom(self.parse_axiom()?)),
                TokenKind::Extern => Some(Item::ExternFn(self.parse_extern_fn()?)),
                TokenKind::Mod => Some(Item::Mod(self.parse_mod_decl()?)),
                TokenKind::Use => Some(Item::Use(self.parse_use_decl()?)),
                _ => {
                    self.error(ParseErrorKind::Expected("item".to_string()));
                    None
                }
            }
        }
    }

    /// Peek past `pub(crate)` to see what token follows.
    fn peek_past_pub_crate(&self) -> TokenKind {
        // Current: pub, +1: (, +2: crate, +3: ), +4: item
        self.peek_n(4).kind
    }

    /// Peek one token ahead.
    fn peek_ahead(&self) -> &crate::token::Token {
        self.peek_n(1)
    }

    /// Parse a module declaration: `mod foo;` or `pub mod foo;`
    fn parse_mod_decl(&mut self) -> Option<ModDecl> {
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
    fn parse_use_decl(&mut self) -> Option<UseDecl> {
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

    pub(super) fn parse_function(&mut self) -> Option<FunctionDef> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        self.expect(TokenKind::Fn)?;

        let name = self.parse_ident()?;
        let type_params = self.parse_optional_type_params();
        let params = self.parse_params()?;

        let return_type = if self.eat(TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        let body = self.parse_block_expr()?;
        let end = body.span().end;

        Some(FunctionDef {
            visibility,
            name,
            type_params,
            params,
            return_type,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_type_item(&mut self) -> Option<Item> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        self.expect(TokenKind::Type)?;

        let name = self.parse_ident()?;
        let type_params = self.parse_optional_type_params();

        if self.eat(TokenKind::Eq) {
            // Could be type alias, ADT, or record type
            if self.check(TokenKind::LBrace) {
                // Record type: type Point = { x: Nat, y: Nat }
                let (record_fields, end) = self.parse_record_type_body()?;
                Some(Item::TypeDef(TypeDef {
                    visibility,
                    name,
                    type_params,
                    body: TypeBody::Record(record_fields),
                    span: Span::new(start, end),
                }))
            } else if self.check(TokenKind::Pipe) || self.check_ident() {
                // ADT with variants
                let variants = self.parse_variants()?;
                let end = variants.last().map_or(name.span.end, |v| v.span.end);
                Some(Item::TypeDef(TypeDef {
                    visibility,
                    name,
                    type_params,
                    body: TypeBody::Sum(variants),
                    span: Span::new(start, end),
                }))
            } else {
                // Type alias
                let ty = self.parse_type()?;
                let end = ty.span().end;
                Some(Item::TypeAlias(TypeAlias {
                    visibility,
                    name,
                    type_params,
                    ty,
                    span: Span::new(start, end),
                }))
            }
        } else if self.check(TokenKind::LBrace) {
            // ADT with braces (Rust-like enum syntax)
            self.expect(TokenKind::LBrace)?;
            let variants = self.parse_variants()?;
            let end = self.expect(TokenKind::RBrace)?.end;
            Some(Item::TypeDef(TypeDef {
                visibility,
                name,
                type_params,
                body: TypeBody::Sum(variants),
                span: Span::new(start, end),
            }))
        } else {
            self.error(ParseErrorKind::Expected(
                "`=` or `{` after type name".to_string(),
            ));
            None
        }
    }

    /// Parse a record type body: `{ field: Type, ... }`
    fn parse_record_type_body(&mut self) -> Option<(Vec<RecordField>, u32)> {
        let _start = self.expect(TokenKind::LBrace)?;

        let mut fields = Vec::new();
        while !self.check(TokenKind::RBrace) && !self.at_eof() {
            let field_start = self.current_span().start;
            let name = self.parse_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let field_end = ty.span().end;

            fields.push(RecordField {
                name,
                ty,
                span: Span::new(field_start, field_end),
            });

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        let end = self.expect(TokenKind::RBrace)?.end;
        Some((fields, end))
    }

    fn parse_struct(&mut self) -> Option<TypeDef> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        self.expect(TokenKind::Struct)?;

        let name = self.parse_ident()?;
        let type_params = self.parse_optional_type_params();

        self.expect(TokenKind::LBrace)?;
        let fields = self.parse_struct_fields()?;
        let end = self.expect(TokenKind::RBrace)?.end;

        // Convert struct to single-variant type def
        let variant = Variant {
            name: name.clone(),
            fields,
            span: Span::new(start, end),
        };

        Some(TypeDef {
            visibility,
            name,
            type_params,
            body: TypeBody::Sum(vec![variant]),
            span: Span::new(start, end),
        })
    }

    fn parse_enum(&mut self) -> Option<TypeDef> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        self.expect(TokenKind::Enum)?;

        let name = self.parse_ident()?;
        let type_params = self.parse_optional_type_params();

        self.expect(TokenKind::LBrace)?;
        let variants = self.parse_enum_variants()?;
        let end = self.expect(TokenKind::RBrace)?.end;

        Some(TypeDef {
            visibility,
            name,
            type_params,
            body: TypeBody::Sum(variants),
            span: Span::new(start, end),
        })
    }

    fn parse_theorem(&mut self) -> Option<TheoremDef> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        // Skip theorem or lemma keyword
        self.advance();

        let name = self.parse_ident()?;
        let type_params = self.parse_optional_type_params();
        let params = if self.check(TokenKind::LParen) {
            self.parse_params()?
        } else {
            Vec::new()
        };

        // Accept both `:` and `->` for the proposition
        if !self.eat(TokenKind::Colon) && !self.eat(TokenKind::Arrow) {
            self.error(ParseErrorKind::Expected(
                "`:` or `->` before proposition".to_string(),
            ));
            return None;
        }
        let prop = self.parse_type()?;

        let body = if self.check(TokenKind::LBrace) {
            self.parse_block_expr()?
        } else if self.eat(TokenKind::Eq) {
            self.parse_expr()?
        } else {
            self.error(ParseErrorKind::Expected(
                "`{` or `=` for proof body".to_string(),
            ));
            return None;
        };

        let end = body.span().end;

        Some(TheoremDef {
            visibility,
            name,
            type_params,
            params,
            prop,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_axiom(&mut self) -> Option<AxiomDef> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        self.expect(TokenKind::Axiom)?;

        let name = self.parse_ident()?;
        let type_params = self.parse_optional_type_params();
        let params = if self.check(TokenKind::LParen) {
            self.parse_params()?
        } else {
            Vec::new()
        };

        // Accept both `:` and `->` for the proposition
        if !self.eat(TokenKind::Colon) && !self.eat(TokenKind::Arrow) {
            self.error(ParseErrorKind::Expected(
                "`:` or `->` before proposition".to_string(),
            ));
            return None;
        }
        let prop = self.parse_type()?;
        let end = prop.span().end;

        Some(AxiomDef {
            visibility,
            name,
            type_params,
            params,
            prop,
            span: Span::new(start, end),
        })
    }

    /// Parse an extern function declaration: `extern fn name(params) -> RetType`
    /// Or with ABI: `extern "C" fn name(params) -> RetType`
    fn parse_extern_fn(&mut self) -> Option<ExternFnDef> {
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
            let span = self.current_span();
            self.errors.push(
                crate::error::ParseError::new(
                    span,
                    crate::error::ParseErrorKind::Expected("`fn` keyword".to_string()),
                )
                .with_expected(vec!["`fn`".to_string()]),
            );
            // Add note about correct syntax
            self.errors.last_mut().map(|e| {
                e.suggestions.push(crate::error::Suggestion::new(
                    Span::new(start, span.end),
                    "extern \"C\" fn name(arg: Type) -> RetType",
                    "extern declarations use function syntax: `extern \"C\" fn name(params) -> ReturnType`",
                ));
            });
            return None;
        }

        if !self.eat(TokenKind::Fn) {
            self.error(ParseErrorKind::Expected(
                "`fn` keyword after `extern` or ABI string".to_string(),
            ));
            return None;
        }

        let name = self.parse_ident()?;

        // Parse parameters
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
}
