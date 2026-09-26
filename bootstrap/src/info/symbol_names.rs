//! The LLVM symbol set one `.tg` function compiles to (ADR 5.8.26b retrospective).
//!
//! Pure string logic, deliberately outside the `codegen` feature gate: the
//! naming convention is a property of the emitter's *rules*, not of a linked
//! LLVM, so it stays testable in the LLVM-free build and is reachable by the
//! coverage/mutation diff gates.
//!
//! **Why this exists.** `.claude/CLAUDE.md` long documented "each `.tg` function
//! compiles to two LLVM symbols". It is up to four kinds, and the omission has
//! a measurable consequence: attributing a `perf` profile to a source function
//! means summing self time across *all* of them, and a reader who knows only
//! `<name>` and `<name>$direct` silently under-counts. Measured during ADR
//! 5.8.26b, where the hot function's samples landed almost entirely in
//! `$direct_mt` — the one kind the docs never mentioned.

/// One symbol a source function compiles to, with the role that explains why
/// a profile might attribute samples to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSymbol {
    /// The mangled symbol as it appears in `nm` output and `perf report`.
    pub symbol: String,
    /// What this symbol is, in one line.
    pub role: &'static str,
}

/// The symbols derivable from a function's name alone.
///
/// Lambdas are NOT here: `<fn>_lambda_N` exists only for functions that
/// actually close over something, so its presence is a fact about the emitted
/// module rather than about the name. `info codegen symbols --by-function`
/// joins these with the emitted lambda map to give the complete set.
pub fn conventional_symbols(fn_name: &str) -> Vec<FunctionSymbol> {
    vec![
        FunctionSymbol {
            symbol: fn_name.to_string(),
            role: "closure-returning wrapper (allocates env, returns {fn_ptr, env_ptr})",
        },
        FunctionSymbol {
            symbol: format!("{fn_name}$direct"),
            role: "direct-call variant (env ptr + args) — the GDB breakpoint target",
        },
        FunctionSymbol {
            symbol: format!("{fn_name}$direct_mt"),
            role:
                "musttail/indirect-buffer callee — where a self-recursive hot loop's samples land",
        },
    ]
}

/// Does `symbol` belong to `fn_name`'s family?
///
/// Used to pick a function's rows out of a flat symbol list. Matching is on
/// the full name plus a known suffix, never a bare prefix: `foo` must not
/// claim `foobar`, and `foo$direct` must not claim `foo$direct_mt`'s samples
/// twice.
pub fn belongs_to(symbol: &str, fn_name: &str) -> bool {
    if symbol == fn_name {
        return true;
    }
    let Some(rest) = symbol.strip_prefix(fn_name) else {
        return false;
    };
    rest == "$direct" || rest == "$direct_mt" || is_lambda_suffix(rest)
}

/// `_lambda_1`, `_lambda_27`, … — and nothing else.
fn is_lambda_suffix(rest: &str) -> bool {
    rest.strip_prefix("_lambda_")
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// Does one emitted symbol-map row belong to `fn_name`?
///
/// A row qualifies two independent ways, and both are needed: the emitter
/// *recorded* it against this source function (`source_name`), or its IR name
/// is a member of the family by the naming convention ([`belongs_to`]). An
/// anonymous closure has no `source_name` but a conventional `ir_name`; a
/// renamed one is the reverse. Narrowing this to a conjunction silently empties
/// the lambda list, which reads as "this function has no closures" rather than
/// as a bug — which is why it is a named function with its own tests rather
/// than an inline predicate in a printer.
pub fn row_belongs_to(source_name: Option<&str>, ir_name: &str, fn_name: &str) -> bool {
    source_name == Some(fn_name) || belongs_to(ir_name, fn_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_conventional_set_is_the_three_name_derived_symbols() {
        let syms = conventional_symbols("import_list_lookup");
        let names: Vec<_> = syms.iter().map(|s| s.symbol.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "import_list_lookup",
                "import_list_lookup$direct",
                "import_list_lookup$direct_mt",
            ]
        );
        // Every entry explains itself — an unlabelled symbol list is what the
        // docs already were.
        assert!(syms.iter().all(|s| !s.role.is_empty()));
    }

    /// `$direct_mt` must be listed. It is the kind CLAUDE.md omitted, and the
    /// kind that carried 56.94% of ADR 5.8.26b's profile.
    #[test]
    fn direct_mt_is_present_because_the_docs_used_to_omit_it() {
        let syms = conventional_symbols("f");
        assert!(syms.iter().any(|s| s.symbol == "f$direct_mt"));
    }

    #[test]
    fn membership_accepts_every_kind() {
        assert!(belongs_to("f", "f"));
        assert!(belongs_to("f$direct", "f"));
        assert!(belongs_to("f$direct_mt", "f"));
        assert!(belongs_to("f_lambda_1", "f"));
        assert!(belongs_to("f_lambda_27", "f"));
    }

    /// A bare prefix match would fold a different function's samples into this
    /// one's total — the exact way a profile silently lies.
    #[test]
    fn membership_rejects_a_longer_unrelated_name() {
        assert!(!belongs_to("foobar", "foo"));
        assert!(!belongs_to("foo_helper", "foo"));
        assert!(!belongs_to("foo$directly", "foo"));
        assert!(!belongs_to("foo_lambda_", "foo"));
        assert!(!belongs_to("foo_lambda_x", "foo"));
        assert!(!belongs_to("other", "foo"));
    }

    /// The reverse direction: a shorter name must not claim a longer one's
    /// family members either.
    #[test]
    fn membership_is_not_symmetric_across_different_functions() {
        assert!(!belongs_to("foo$direct", "foobar"));
        assert!(belongs_to("foobar$direct", "foobar"));
    }

    /// Each disjunct alone must qualify a row. Turning this into a conjunction
    /// empties the lambda list, which reads as "no closures here" rather than
    /// as a defect.
    #[test]
    fn a_row_qualifies_by_recorded_name_or_by_convention_independently() {
        // Recorded against the function, IR name unrecognisable on its own.
        assert!(row_belongs_to(Some("f"), "__anon_7", "f"));
        // Conventional IR name, nothing recorded.
        assert!(row_belongs_to(None, "f_lambda_2", "f"));
        // Both.
        assert!(row_belongs_to(Some("f"), "f_lambda_2", "f"));
    }

    /// A row belonging to a DIFFERENT function must not be pulled in — that
    /// would attribute another function's closures to this one.
    #[test]
    fn a_row_for_another_function_does_not_qualify() {
        assert!(!row_belongs_to(Some("g"), "g_lambda_1", "f"));
        assert!(!row_belongs_to(None, "__anon_7", "f"));
        // The equality is on the WHOLE recorded name, not a prefix of it.
        assert!(!row_belongs_to(Some("foobar"), "__anon_1", "foo"));
    }
}
