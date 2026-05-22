//! Table and JSON output for fold/unfold consistency results.

use super::AdtFoldResult;

pub fn print_table(results: &[AdtFoldResult], verbose: bool) {
    print!("{}", format_table(results, verbose));
}

pub fn print_json(results: &[AdtFoldResult]) {
    print!("{}", format_json(results));
}

/// Escape a string for JSON output (handles `"`, `\`, and control chars).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn format_table(results: &[AdtFoldResult], verbose: bool) -> String {
    let mut out = String::new();
    out.push_str("Fold/Unfold Consistency Check\n");
    out.push_str("═════════════════════════════\n");
    out.push('\n');
    out.push_str(&format!(
        "  {:<24} {:>5} {:>4} {:>4} {:>6} {:>6}  {}\n",
        "ADT", "Recur", "μ", "Fold", "Unfold", "Status", ""
    ));
    out.push_str(&format!(
        "  {:<24} {:>5} {:>4} {:>4} {:>6} {:>6}  {}\n",
        "───", "─────", "──", "────", "──────", "──────", ""
    ));

    for r in results {
        let status = if r.consistent { "✓" } else { "✗" };
        out.push_str(&format!(
            "  {:<24} {:>5} {:>4} {:>4} {:>6} {:>6}  {}\n",
            r.name,
            if r.is_recursive { "yes" } else { "no" },
            if r.has_mu_binder { "yes" } else { "no" },
            r.fold_sites,
            r.unfold_sites,
            status,
            if !r.consistent { "INCONSISTENT" } else { "" },
        ));
        if !r.consistent && verbose {
            for d in &r.disagreements {
                out.push_str(&format!("    → {d}\n"));
            }
            if let Some(ref group) = r.scc_group {
                out.push_str(&format!("    → mutual group: {{{}}}\n", group.join(", ")));
            }
        }
    }

    let total = results.len();
    let pass = results.iter().filter(|r| r.consistent).count();
    let fail = total - pass;
    out.push('\n');
    out.push_str(&format!(
        "Total: {total} ADT(s), {pass} consistent, {fail} inconsistent\n",
    ));
    out
}

fn format_json(results: &[AdtFoldResult]) -> String {
    let entries: Vec<String> = results
        .iter()
        .map(|r| {
            let group = match &r.scc_group {
                Some(g) => format!(
                    "[{}]",
                    g.iter()
                        .map(|s| format!("\"{}\"", json_escape(s)))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                None => "null".to_string(),
            };
            let disag = r
                .disagreements
                .iter()
                .map(|d| format!("\"{}\"", json_escape(d)))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "    {{\"name\": \"{}\", \"recursive\": {}, \"mu_binder\": {}, \"fold_sites\": {}, \"unfold_sites\": {}, \"consistent\": {}, \"scc_group\": {}, \"disagreements\": [{}]}}",
                json_escape(&r.name), r.is_recursive, r.has_mu_binder, r.fold_sites, r.unfold_sites, r.consistent, group, disag
            )
        })
        .collect();

    let status = if results.iter().all(|r| r.consistent) {
        "pass"
    } else {
        "fail"
    };
    format!(
        "{{\"adts\": [\n{}\n  ], \"status\": \"{}\"}}\n",
        entries.join(",\n"),
        status,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(name: &str, consistent: bool) -> AdtFoldResult {
        AdtFoldResult {
            name: name.to_string(),
            is_recursive: !consistent,
            has_mu_binder: !consistent,
            fold_sites: if consistent { 0 } else { 2 },
            unfold_sites: if consistent { 0 } else { 1 },
            consistent,
            scc_group: None,
            disagreements: if consistent {
                vec![]
            } else {
                vec!["test disagreement".to_string()]
            },
        }
    }

    // ─────────────────────────────────────────────────────────────────
    // json_escape
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn json_escape_plain_string() {
        assert_eq!(json_escape("hello"), "hello");
    }

    #[test]
    fn json_escape_quotes_and_backslash() {
        assert_eq!(json_escape(r#"say "hi" \ there"#), r#"say \"hi\" \\ there"#,);
    }

    #[test]
    fn json_escape_control_chars() {
        assert_eq!(json_escape("a\nb\tc"), "a\\nb\\tc");
    }

    // ─────────────────────────────────────────────────────────────────
    // format_table
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn table_header_present() {
        let out = format_table(&[], false);
        assert!(out.contains("Fold/Unfold Consistency Check"));
        assert!(out.contains("Total: 0 ADT(s), 0 consistent, 0 inconsistent"));
    }

    #[test]
    fn table_consistent_row() {
        let results = vec![make_result("Color", true)];
        let out = format_table(&results, false);
        assert!(out.contains("Color"));
        assert!(out.contains("✓"));
        assert!(!out.contains("INCONSISTENT"));
        assert!(out.contains("1 consistent, 0 inconsistent"));
    }

    #[test]
    fn table_inconsistent_row() {
        let results = vec![make_result("BadAdt", false)];
        let out = format_table(&results, false);
        assert!(out.contains("BadAdt"));
        assert!(out.contains("✗"));
        assert!(out.contains("INCONSISTENT"));
        assert!(out.contains("0 consistent, 1 inconsistent"));
    }

    #[test]
    fn table_verbose_shows_disagreements() {
        let results = vec![make_result("BadAdt", false)];
        let out = format_table(&results, true);
        assert!(out.contains("→ test disagreement"));
    }

    #[test]
    fn table_non_verbose_hides_disagreements() {
        let results = vec![make_result("BadAdt", false)];
        let out = format_table(&results, false);
        assert!(!out.contains("→ test disagreement"));
    }

    #[test]
    fn table_verbose_shows_scc_group() {
        let mut r = make_result("Tree", false);
        r.scc_group = Some(vec!["Tree".into(), "Forest".into()]);
        let out = format_table(&[r], true);
        assert!(out.contains("mutual group: {Tree, Forest}"));
    }

    // ─────────────────────────────────────────────────────────────────
    // format_json
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn json_empty_results() {
        let out = format_json(&[]);
        assert!(out.contains("\"status\": \"pass\""));
        assert!(out.contains("\"adts\": ["));
    }

    #[test]
    fn json_consistent_entry() {
        let results = vec![make_result("Color", true)];
        let out = format_json(&results);
        assert!(out.contains("\"name\": \"Color\""));
        assert!(out.contains("\"consistent\": true"));
        assert!(out.contains("\"status\": \"pass\""));
    }

    #[test]
    fn json_inconsistent_sets_fail() {
        let results = vec![make_result("Bad", false)];
        let out = format_json(&results);
        assert!(out.contains("\"consistent\": false"));
        assert!(out.contains("\"status\": \"fail\""));
    }

    #[test]
    fn json_scc_group_rendered() {
        let mut r = make_result("Tree", true);
        r.scc_group = Some(vec!["Tree".into(), "Forest".into()]);
        let out = format_json(&[r]);
        assert!(out.contains("[\"Tree\", \"Forest\"]"));
    }

    #[test]
    fn json_null_scc_group() {
        let r = make_result("Color", true);
        let out = format_json(&[r]);
        assert!(out.contains("\"scc_group\": null"));
    }

    #[test]
    fn json_escapes_special_chars_in_name() {
        let r = make_result("Adt\"Quote", true);
        let out = format_json(&[r]);
        assert!(out.contains("Adt\\\"Quote"));
    }

    #[test]
    fn json_escapes_disagreement_strings() {
        let mut r = make_result("Bad", false);
        r.disagreements = vec!["has \"quoted\" text".to_string()];
        let out = format_json(&[r]);
        assert!(out.contains(r#"has \"quoted\" text"#));
    }

    #[test]
    fn json_multiple_entries_comma_separated() {
        let results = vec![make_result("A", true), make_result("B", true)];
        let out = format_json(&results);
        assert!(out.contains("},\n"));
    }
}
