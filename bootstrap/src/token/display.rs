//! Token display strings and keyword lookup.
//!
//! Extracted from `mod.rs` to reduce file size. Contains the `token_strings!`
//! macro, its invocation (generating `as_str`, `keyword_from_str`, `Display`),
//! and all keyword/display string mappings.

use super::TokenKind;
use std::fmt;

/// Generates `TokenKind::as_str()` and the free function `keyword_from_str()`.
///
/// Keyword entries (`keyword: Variant => "str"`) participate in both
/// `as_str()` (variant→string) and `keyword_from_str()` (string→variant).
///
/// Display-only entries (`display: Variant => "str"`) participate in
/// `as_str()` only.
macro_rules! token_strings {
    (
        $(keyword: $kw_variant:ident => $kw_str:expr),* $(,)?
        ;
        $(display: $disp_variant:ident => $disp_str:expr),* $(,)?
    ) => {
        impl TokenKind {
            /// Get the display string for this token kind.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(TokenKind::$kw_variant => $kw_str,)*
                    $(TokenKind::$disp_variant => $disp_str,)*
                }
            }
        }

        /// Look up a keyword from its string representation.
        #[must_use]
        pub fn keyword_from_str(s: &str) -> Option<TokenKind> {
            match s {
                $($kw_str => Some(TokenKind::$kw_variant),)*
                _ => None,
            }
        }

        impl fmt::Display for TokenKind {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.as_str())
            }
        }
    };
}

token_strings! {
    // ── Core keywords ──────────────────────────────────────────
    keyword: Fn       => "fn",
    keyword: Let      => "let",
    keyword: If       => "if",
    keyword: Else     => "else",
    keyword: Match    => "match",
    keyword: Return   => "return",
    keyword: TryBlock => "try",
    keyword: Type     => "type",
    keyword: Struct   => "struct",
    keyword: Enum     => "enum",
    keyword: True     => "true",
    keyword: False    => "false",

    // ── Proof keywords ─────────────────────────────────────────
    keyword: Theorem  => "theorem",
    keyword: Lemma    => "lemma",
    keyword: Axiom    => "axiom",
    keyword: By       => "by",
    keyword: Have     => "have",
    keyword: Show     => "show",
    keyword: Assume   => "assume",
    keyword: Forall   => "forall",
    keyword: Exists   => "exists",
    keyword: Prop     => "Prop",
    keyword: Sorry    => "sorry",
    keyword: Refl     => "refl",
    keyword: Subst    => "subst",
    keyword: Sym      => "sym",
    keyword: Trans    => "trans",
    keyword: Cong     => "cong",
    keyword: NatInd   => "natind",
    keyword: NatRec   => "natrec",

    // ── Type keywords ──────────────────────────────────────────
    keyword: Bool     => "Bool",
    keyword: Nat      => "Nat",
    keyword: Unit     => "Unit",
    keyword: Void     => "Void",

    // ── Module keywords ────────────────────────────────────────
    keyword: Mod      => "mod",
    keyword: Pub      => "pub",

    // ── Reserved keywords ──────────────────────────────────────
    keyword: Async    => "async",
    keyword: Await    => "await",
    keyword: Use      => "use",
    keyword: Impl     => "impl",
    keyword: Trait    => "trait",
    keyword: Where    => "where",
    keyword: SelfLower => "self",
    keyword: SelfUpper => "Self",
    keyword: Mut      => "mut",
    keyword: Ref      => "ref",
    keyword: Move     => "move",
    keyword: Loop     => "loop",
    keyword: While    => "while",
    keyword: For      => "for",
    keyword: In       => "in",
    keyword: Break    => "break",
    keyword: Continue => "continue",
    keyword: Const    => "const",
    keyword: Static   => "static",
    keyword: Extern   => "extern",
    keyword: Crate    => "crate",
    keyword: Super    => "super",
    keyword: Dyn      => "dyn",
    keyword: Unsafe   => "unsafe",
    keyword: As       => "as",
    ;
    // ── Literals & identifiers (display only) ──────────────────
    display: IntLiteral    => "<int>",
    display: StringLiteral => "<string>",
    display: CharLiteral   => "<char>",
    display: Ident         => "<identifier>",
    display: Underscore    => "_",

    // ── Delimiters ─────────────────────────────────────────────
    display: LParen    => "(",
    display: RParen    => ")",
    display: LBrace    => "{",
    display: RBrace    => "}",
    display: LBracket  => "[",
    display: RBracket  => "]",
    display: Lt        => "<",
    display: Gt        => ">",

    // ── Punctuation ────────────────────────────────────────────
    display: Comma     => ",",
    display: Semi      => ";",
    display: Colon     => ":",
    display: ColonColon => "::",
    display: Dot       => ".",
    display: DotDot    => "..",
    display: DotDotDot => "...",
    display: FatArrow  => "=>",
    display: Arrow     => "->",
    display: At        => "@",
    display: Hash      => "#",
    display: Question  => "?",

    // ── Operators ──────────────────────────────────────────────
    display: Eq        => "=",
    display: EqEq      => "==",
    display: Ne        => "!=",
    display: Le        => "<=",
    display: Ge        => ">=",
    display: Plus      => "+",
    display: Minus     => "-",
    display: Star      => "*",
    display: Slash     => "/",
    display: Percent   => "%",
    display: Amp       => "&",
    display: AmpAmp    => "&&",
    display: Pipe      => "|",
    display: PipePipe  => "||",
    display: PipeRight => "|>",
    display: Bang      => "!",
    display: Caret     => "^",
    display: Tilde     => "~",
    display: PlusPlus  => "++",

    // ── Trivia ─────────────────────────────────────────────────
    display: Whitespace   => "<whitespace>",
    display: LineComment  => "<line comment>",
    display: BlockComment => "<block comment>",
    display: DocComment   => "<doc comment>",

    // ── Special ────────────────────────────────────────────────
    display: Eof       => "<eof>",
    display: Error     => "<error>",
}
