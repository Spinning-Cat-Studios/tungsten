//! Shared text-level LLVM IR parsing primitives for the `ir_checks` family
//! (ADR 2.7.26b friction paydown — previously duplicated in
//! `check_indirect_buffers` and `check_merge_truncation`).

/// Extract the quoted or bare function name from a `define … @NAME(` header.
pub(crate) fn parse_define_name(def: &str) -> String {
    let Some(at) = def.find('@') else {
        return String::new();
    };
    let rest = &def[at + 1..];
    if let Some(stripped) = rest.strip_prefix('"') {
        stripped
            .find('"')
            .map_or_else(String::new, |end| stripped[..end].to_string())
    } else {
        rest.split(['(', ' ']).next().unwrap_or("").to_string()
    }
}

/// One parsed LLVM function: name, `define` header line, and body lines
/// (up to but excluding the closing `}`).
pub(crate) struct IrFunc<'a> {
    pub(crate) name: String,
    pub(crate) header: &'a str,
    pub(crate) body: Vec<&'a str>,
}

/// Split a module into functions, capturing each `define` header + body lines.
/// (Promoted from `check_indirect_buffers::parse` for reuse by
/// `check_sret_stores` — ADR 3.7.26d "extend, don't fork".)
pub(crate) fn split_ir_functions(text: &str) -> Vec<IrFunc<'_>> {
    let mut funcs = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("define ") {
            continue;
        }
        let name = parse_define_name(trimmed);
        let mut body = Vec::new();
        for l in lines.by_ref() {
            if l.trim_start() == "}" {
                break;
            }
            body.push(l);
        }
        funcs.push(IrFunc {
            name,
            header: trimmed,
            body,
        });
    }
    funcs
}

/// One parsed `call`/`invoke` instruction: what it returns and what it calls.
pub(crate) struct CallSite<'a> {
    /// The return-type text between the `call` keyword and the callee —
    /// `void`, `{ ptr, ptr }`, `i64 (ptr, ...)`.
    pub(crate) return_type: &'a str,
    /// The callee: `@foo`, `@"foo$direct"`, `%fptr`, or the literal `null`.
    pub(crate) callee: &'a str,
}

/// Parse a `call`/`invoke` instruction, or `None` when `line` is not one.
///
/// Reading the callee off its **position** rather than from anywhere in the
/// line is the whole point (ADR 28.7.26e D2). `null-calls` used to ask whether
/// `null(` appeared anywhere after the `call` keyword, so every call to a
/// function whose *name* ends in `null` — `@cstring_is_null(` — read as a null
/// callee; all 15 of its corpus findings were that. The rule here also survives
/// `tail`/`musttail` prefixes, calling conventions, return-value attributes,
/// `addrspace(N)` inside the return type, and the explicit varargs
/// function-type spelling `call i64 (ptr, ...) @printf(…)` — none of which
/// "the token after the return type" survives.
pub(crate) fn parse_call(line: &str) -> Option<CallSite<'_>> {
    let after = after_call_keyword(line)?;
    // The callee is the token immediately preceding the argument list. Try each
    // `(` in turn and take the first whose preceding token *looks* like a
    // callee, so parenthesised return-type syntax is skipped rather than
    // mistaken for the argument list.
    for (paren, _) in after.match_indices('(') {
        let head = after[..paren].trim_end();
        let callee_start = head.rfind(char::is_whitespace).map_or(0, |space| space + 1);
        let callee = &head[callee_start..];
        if is_callee_token(callee) {
            return Some(CallSite {
                return_type: after[..callee_start].trim(),
                callee,
            });
        }
    }
    None
}

/// The callee of a `call`/`invoke` instruction — [`parse_call`] when the return
/// type does not matter.
pub(crate) fn callee_of(line: &str) -> Option<&str> {
    parse_call(line).map(|call| call.callee)
}

/// True for the three callee spellings LLVM allows in a direct/indirect call:
/// a global symbol, an SSA function pointer, or the `null` literal.
fn is_callee_token(token: &str) -> bool {
    token.starts_with('@') || token.starts_with('%') || token == "null"
}

/// The text following the `call`/`invoke` keyword, matched at a token boundary
/// so the `call` inside an SSA name (`%call = call i64 @f()`) or an identifier
/// (`%recall`) is not mistaken for the keyword.
fn after_call_keyword(line: &str) -> Option<&str> {
    ["call ", "invoke "]
        .iter()
        .filter_map(|kw| {
            line.match_indices(kw)
                .find(|(i, _)| *i == 0 || !is_ident_byte(line.as_bytes()[i - 1]))
                .map(|(i, _)| (i, &line[i + kw.len()..]))
        })
        .min_by_key(|(i, _)| *i)
        .map(|(_, rest)| rest)
}

/// Bytes that can appear inside an LLVM identifier (including its `%`/`@`
/// sigils and the `$` of `foo$direct`), i.e. bytes that mean a `call ` match
/// starting here is part of a longer name rather than the keyword.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$' | b'%' | b'@' | b'-')
}

/// Split a parameter/argument list at TOP-LEVEL commas only — commas inside
/// nested `sret({ i64, i64 })` / `byval([4 x i8])` type attributes must not
/// split an entry (a naive `split(',')` silently drops the sret buffer from
/// the tracked set — the parser-drift class ADR 2.7.26b T5a guards against).
pub(crate) fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '{' | '[' | '<' => depth += 1,
            ')' | '}' | ']' | '>' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_name_quoted_and_bare() {
        assert_eq!(
            parse_define_name(r#"define void @"scan_loop$direct_mt"(ptr %0) {"#),
            "scan_loop$direct_mt"
        );
        assert_eq!(parse_define_name("define i64 @main(ptr %0) {"), "main");
        assert_eq!(parse_define_name("no at-sign here"), "");
    }

    #[test]
    fn top_level_commas_respect_nesting() {
        // The sret-attribute regression shape: the inner comma must not split.
        let params = "ptr sret({ i64, i64 }) align 8 %0, ptr %1, i64 %2";
        let parts = split_top_level_commas(params);
        assert_eq!(parts.len(), 3, "{parts:?}");
        assert!(parts[0].contains("sret({ i64, i64 })"));

        // Arrays and vectors nest too.
        assert_eq!(split_top_level_commas("[4 x i8], <2 x i64>, i1").len(), 3);
        // No commas → one entry.
        assert_eq!(split_top_level_commas("i64").len(), 1);
    }

    /// The defect ADR 28.7.26e D2 fixes, both directions. The first line is
    /// copied verbatim from the emitted corpus
    /// (`target/ll-audit/driver/ffi/io/mkdir_p.ll:50`) — it was one of the 15
    /// false `null-calls` findings.
    #[test]
    fn callee_position_separates_a_null_callee_from_a_name_ending_in_null() {
        assert_eq!(
            callee_of("  %call7 = call i1 @cstring_is_null(ptr null, i64 %thunk_call)"),
            Some("@cstring_is_null"),
            "the callee is the named function, not the `null` inside its name"
        );
        assert_eq!(
            callee_of("  %1 = call i64 null(ptr null)"),
            Some("null"),
            "a genuine null callee still resolves"
        );
    }

    #[test]
    fn callee_survives_prefixes_return_types_and_indirection() {
        for (line, expected) in [
            // musttail prefix + quoted symbol + attributed sret argument.
            (
                r#"  musttail call void @"ok$direct_mt"(ptr noalias sret({ i64, i64 }) %0)"#,
                Some(r#"@"ok$direct_mt""#),
            ),
            // Aggregate return type — the braces carry commas, not parens.
            (
                "  %c = call { ptr, ptr } @wrapper(ptr null, i64 %1)",
                Some("@wrapper"),
            ),
            // Explicit varargs function type: `(ptr, ...)` is NOT the arg list.
            (
                "  %n = call i64 (ptr, ...) @printf(ptr %fmt, i64 %v)",
                Some("@printf"),
            ),
            // Parenthesised return-type qualifier.
            ("  %p = call ptr addrspace(1) @alloc(i64 8)", Some("@alloc")),
            // Indirect call through a function pointer.
            ("  call void %fptr(i64 %1)", Some("%fptr")),
            // `invoke` is a call too.
            (
                "  invoke void @bar(i64 %0) to label %cont unwind label %cleanup",
                Some("@bar"),
            ),
            // The `call` inside an SSA name is not the keyword…
            ("  %call = call i64 @f(i64 %0)", Some("@f")),
            // …nor is the one inside a longer identifier.
            ("  %recall = add i64 %0, 1", None),
            // Not a call at all.
            ("  store ptr null, ptr %1", None),
            ("declare ptr @malloc(i64)", None),
        ] {
            assert_eq!(callee_of(line), expected, "{line}");
        }
    }

    /// The return type is the discriminator `wrapper-self-calls` needs: a
    /// closure-returning wrapper yields `{ ptr, ptr }`, a saturated entry
    /// yields its result aggregate (ADR 28.7.26e §2.3).
    #[test]
    fn call_return_type_is_recovered_alongside_the_callee() {
        for (line, return_type, callee) in [
            (
                "  %c = call { ptr, ptr } @wrapper(ptr null, i64 %1)",
                "{ ptr, ptr }",
                "@wrapper",
            ),
            (
                "  %call = call { i32, [16 x i8] } @list_last_I_6String(ptr null, ptr %snd)",
                "{ i32, [16 x i8] }",
                "@list_last_I_6String",
            ),
            (
                r#"  musttail call void @"ok$direct_mt"(ptr %0)"#,
                "void",
                r#"@"ok$direct_mt""#,
            ),
            (
                "  %n = call i64 (ptr, ...) @printf(ptr %fmt)",
                "i64 (ptr, ...)",
                "@printf",
            ),
        ] {
            let call = parse_call(line).unwrap_or_else(|| panic!("not parsed: {line}"));
            assert_eq!(call.return_type, return_type, "{line}");
            assert_eq!(call.callee, callee, "{line}");
        }
        assert!(parse_call("  ret void").is_none());
    }
}
