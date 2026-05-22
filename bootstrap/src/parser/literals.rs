//! Literal parsing utilities for the parser.
//!
//! Handles string escape sequences and integer literal parsing.

use super::Parser;

/// Process a `\xNN` hex escape sequence, appending to `result`.
pub(crate) fn unescape_hex(chars: &mut std::iter::Peekable<std::str::Chars>, result: &mut String) {
    let hex: String = chars.take(2).collect();
    if hex.len() == 2 {
        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
            result.push(byte as char);
        } else {
            result.push_str("\\x");
            result.push_str(&hex);
        }
    } else {
        result.push_str("\\x");
        result.push_str(&hex);
    }
}

/// Process a `\u{NNNNNN}` unicode escape sequence, appending to `result`.
pub(crate) fn unescape_unicode(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    result: &mut String,
) {
    if chars.peek() != Some(&'{') {
        result.push_str("\\u");
        return;
    }
    chars.next(); // consume '{'
    let mut hex = String::new();
    while let Some(&c) = chars.peek() {
        if c == '}' {
            chars.next(); // consume '}'
            break;
        } else if c.is_ascii_hexdigit() && hex.len() < 6 {
            hex.push(c);
            chars.next();
        } else {
            break;
        }
    }
    if !hex.is_empty() {
        if let Ok(code) = u32::from_str_radix(&hex, 16) {
            if let Some(ch) = char::from_u32(code) {
                result.push(ch);
                return;
            }
        }
        result.push_str("\\u{");
        result.push_str(&hex);
        result.push('}');
    } else {
        result.push_str("\\u{}");
    }
}

impl<'a> Parser<'a> {
    pub(crate) fn parse_int_literal(&self, text: &str) -> u64 {
        let text = text.replace('_', "");
        if text.starts_with("0x") || text.starts_with("0X") {
            u64::from_str_radix(&text[2..], 16).unwrap_or(0)
        } else if text.starts_with("0o") || text.starts_with("0O") {
            u64::from_str_radix(&text[2..], 8).unwrap_or(0)
        } else if text.starts_with("0b") || text.starts_with("0B") {
            u64::from_str_radix(&text[2..], 2).unwrap_or(0)
        } else {
            text.parse().unwrap_or(0)
        }
    }

    pub(crate) fn unescape_string(&self, s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('r') => result.push('\r'),
                    Some('t') => result.push('\t'),
                    Some('\\') => result.push('\\'),
                    Some('\'') => result.push('\''),
                    Some('"') => result.push('"'),
                    Some('0') => result.push('\0'),
                    Some('x') => unescape_hex(&mut chars, &mut result),
                    Some('u') => unescape_unicode(&mut chars, &mut result),
                    Some(c) => {
                        result.push('\\');
                        result.push(c);
                    }
                    None => result.push('\\'),
                }
            } else {
                result.push(c);
            }
        }
        result
    }
}
