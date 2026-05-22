//! S-expression tokenizer, parser, and structural comparator for Core IR.
//!
//! Parses the Display output of `tungsten_core` `Type` and `Term` values
//! into tree structures for structural comparison. Handles parenthesized
//! `(...)` and bracketed `[...]` groups.

/// A node in the S-expression tree for structural comparison.
#[derive(Debug, Clone, PartialEq)]
pub enum SExpr {
    /// Leaf token: identifier, number, operator, keyword, or string literal.
    Atom(String),
    /// Parenthesized group: `( child₁ child₂ ... )`.
    Paren(Vec<SExpr>),
    /// Bracketed group: `[ child₁ child₂ ... ]`.
    Bracket(Vec<SExpr>),
}

impl SExpr {
    /// Reconstruct a display string from this S-expression.
    pub fn display(&self) -> String {
        match self {
            SExpr::Atom(s) => s.clone(),
            SExpr::Paren(children) => {
                let inner: Vec<String> = children.iter().map(SExpr::display).collect();
                format!("({})", inner.join(" "))
            }
            SExpr::Bracket(children) => {
                let inner: Vec<String> = children.iter().map(SExpr::display).collect();
                format!("[{}]", inner.join(" "))
            }
        }
    }

    /// Truncated display for use in divergence reports.
    pub fn summary(&self, max_len: usize) -> String {
        let full = self.display();
        if full.len() <= max_len {
            full
        } else {
            let end = full
                .char_indices()
                .take_while(|(i, _)| *i < max_len)
                .last()
                .map_or(0, |(i, c)| i + c.len_utf8());
            format!("{}...", &full[..end])
        }
    }
}

// ── Tokenizer ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    Atom(String),
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            '(' => {
                tokens.push(Token::OpenParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::CloseParen);
                chars.next();
            }
            '[' => {
                tokens.push(Token::OpenBracket);
                chars.next();
            }
            ']' => {
                tokens.push(Token::CloseBracket);
                chars.next();
            }
            '"' => {
                tokens.push(Token::Atom(tokenize_string_literal(&mut chars)));
            }
            c if c.is_whitespace() => {
                chars.next();
            }
            _ => {
                tokens.push(Token::Atom(tokenize_bare_atom(&mut chars)));
            }
        }
    }

    tokens
}

/// Consume a string literal from `"` through the closing `"`, handling escapes.
fn tokenize_string_literal(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    s.push('"');
    chars.next(); // consume opening "
    let mut escaped = false;
    while let Some(&c) = chars.peek() {
        s.push(c);
        chars.next();
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
        } else if c == '"' {
            break;
        }
    }
    s
}

/// Consume a bare atom (no whitespace or grouping delimiters).
fn tokenize_bare_atom(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut atom = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '"') {
            break;
        }
        atom.push(c);
        chars.next();
    }
    atom
}

// ── Parser ─────────────────────────────────────────────────────────────────

/// Parse a type or term string into a list of top-level S-expressions.
pub fn parse_sexpr(input: &str) -> Vec<SExpr> {
    let tokens = tokenize(input);
    let (exprs, _) = parse_children(&tokens, 0, None);
    exprs
}

#[derive(Clone, Copy)]
enum CloseDelim {
    Paren,
    Bracket,
}

fn parse_children(
    tokens: &[Token],
    start: usize,
    close: Option<CloseDelim>,
) -> (Vec<SExpr>, usize) {
    let mut result = Vec::new();
    let mut i = start;

    while i < tokens.len() {
        match &tokens[i] {
            Token::OpenParen => {
                let (children, end) = parse_children(tokens, i + 1, Some(CloseDelim::Paren));
                result.push(SExpr::Paren(children));
                i = end;
            }
            Token::OpenBracket => {
                let (children, end) = parse_children(tokens, i + 1, Some(CloseDelim::Bracket));
                result.push(SExpr::Bracket(children));
                i = end;
            }
            Token::CloseParen => {
                if matches!(close, Some(CloseDelim::Paren)) {
                    return (result, i + 1);
                }
                i += 1; // skip unexpected
            }
            Token::CloseBracket => {
                if matches!(close, Some(CloseDelim::Bracket)) {
                    return (result, i + 1);
                }
                i += 1; // skip unexpected
            }
            Token::Atom(s) => {
                result.push(SExpr::Atom(s.clone()));
                i += 1;
            }
        }
    }

    (result, i)
}

// ── Structural Diff ────────────────────────────────────────────────────────

/// A divergence point found during structural comparison of two S-expressions.
#[derive(Debug, Clone)]
pub struct Divergence {
    /// Breadcrumb path from root to the divergence point.
    pub path: Vec<PathSegment>,
    /// Nesting depth at which the divergence was found.
    pub depth: usize,
    /// Left-side expression at the divergence point.
    pub left: String,
    /// Right-side expression at the divergence point.
    pub right: String,
}

/// One segment in the path breadcrumb trail.
#[derive(Debug, Clone)]
pub struct PathSegment {
    /// Child index within the parent group.
    #[allow(dead_code)] // Used by JSON output; text output uses label
    pub index: usize,
    /// Human-readable label derived from the expression head.
    pub label: String,
}

const SUMMARY_LEN: usize = 120;

/// Mutable state threaded through the recursive structural diff.
struct DiffState {
    path: Vec<PathSegment>,
    out: Vec<Divergence>,
    max: usize,
}

/// Compare two S-expression sequences, returning up to `max` divergence points.
pub fn structural_diff(a: &[SExpr], b: &[SExpr], max: usize) -> Vec<Divergence> {
    let mut state = DiffState {
        path: Vec::new(),
        out: Vec::new(),
        max,
    };
    diff_children(a, b, &mut state, 0);
    state.out
}

fn diff_children(a: &[SExpr], b: &[SExpr], state: &mut DiffState, depth: usize) {
    let min_len = a.len().min(b.len());
    for i in 0..min_len {
        if state.out.len() >= state.max {
            return;
        }
        state.path.push(PathSegment {
            index: i,
            label: head_label(&a[i]),
        });
        diff_nodes(&a[i], &b[i], state, depth + 1);
        state.path.pop();
    }

    // Report extra children from the longer side.
    if a.len() > b.len() {
        for (i, item) in a.iter().enumerate().skip(min_len) {
            if state.out.len() >= state.max {
                return;
            }
            state.out.push(Divergence {
                path: state.path.clone(),
                depth,
                left: format!("child {i}: {}", item.summary(SUMMARY_LEN)),
                right: "(absent)".to_string(),
            });
        }
    } else if b.len() > a.len() {
        for (i, item) in b.iter().enumerate().skip(min_len) {
            if state.out.len() >= state.max {
                return;
            }
            state.out.push(Divergence {
                path: state.path.clone(),
                depth,
                left: "(absent)".to_string(),
                right: format!("child {i}: {}", item.summary(SUMMARY_LEN)),
            });
        }
    }
}

fn diff_nodes(a: &SExpr, b: &SExpr, state: &mut DiffState, depth: usize) {
    if state.out.len() >= state.max {
        return;
    }

    match (a, b) {
        (SExpr::Atom(va), SExpr::Atom(vb)) if va == vb => {}
        (SExpr::Atom(va), SExpr::Atom(vb)) => {
            state.out.push(Divergence {
                path: state.path.clone(),
                depth,
                left: va.clone(),
                right: vb.clone(),
            });
        }
        (SExpr::Paren(ca), SExpr::Paren(cb)) => {
            diff_children(ca, cb, state, depth);
        }
        (SExpr::Bracket(ca), SExpr::Bracket(cb)) => {
            diff_children(ca, cb, state, depth);
        }
        _ => {
            // Different node kinds (Atom vs Paren, Paren vs Bracket, etc.)
            state.out.push(Divergence {
                path: state.path.clone(),
                depth,
                left: a.summary(SUMMARY_LEN),
                right: b.summary(SUMMARY_LEN),
            });
        }
    }
}

/// Derive a label from an S-expression for use in path breadcrumbs.
fn head_label(expr: &SExpr) -> String {
    match expr {
        SExpr::Atom(s) => {
            if s.len() <= 30 {
                s.clone()
            } else {
                let end = s
                    .char_indices()
                    .take_while(|(i, _)| *i < 27)
                    .last()
                    .map_or(0, |(i, c)| i + c.len_utf8());
                format!("{}...", &s[..end])
            }
        }
        SExpr::Paren(children) => group_head_label(children, "(", ")"),
        SExpr::Bracket(children) => group_head_label(children, "[", "]"),
    }
}

fn group_head_label(children: &[SExpr], open: &str, close: &str) -> String {
    if let Some(SExpr::Atom(h)) = children.first() {
        if h.len() <= 20 {
            return format!("{open}{h} ...{close}");
        }
    }
    format!("{open}...{close}")
}

/// Format a divergence path as a `→`-separated breadcrumb string.
pub fn format_path(path: &[PathSegment]) -> String {
    if path.is_empty() {
        return "root".to_string();
    }
    path.iter()
        .map(|s| s.label.as_str())
        .collect::<Vec<_>>()
        .join(" → ")
}
