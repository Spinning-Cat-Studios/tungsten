//! Rendering the builtin listing, as values so they are assertable.

use std::fmt::Write as _;

/// One name's row in the listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    /// Intercepted by `try_elab_special_application`.
    pub bootstrap: bool,
    /// Intercepted by `synth_app`.
    pub selfhost: bool,
    /// The declared reason slug, when this row is asymmetric and accounted for.
    pub declared: Option<&'static str>,
}

impl Row {
    /// Do the two compilers disagree about this name?
    #[must_use]
    pub fn is_asymmetric(&self) -> bool {
        self.bootstrap != self.selfhost
    }

    /// The verdict cell.
    #[must_use]
    pub fn verdict(&self) -> String {
        if !self.is_asymmetric() {
            return "both".to_string();
        }
        match self.declared {
            Some(slug) => format!("ASYMMETRIC (declared: {slug})"),
            None => "ASYMMETRIC — UNDECLARED".to_string(),
        }
    }
}

fn tick(on: bool) -> &'static str {
    if on {
        "✓"
    } else {
        "·"
    }
}

/// The whole listing.
///
/// **`0 names` and `0 asymmetries` do not render alike** (ADR 20.8.26c AC6). An
/// empty union means the constants are empty, which is a fault in this command
/// and not a clean bill of health for the compilers — so it says so instead of
/// printing a tidy zero.
#[must_use]
pub fn render_listing(rows: &[Row], bootstrap_total: usize, selfhost_total: usize) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Bare names intercepted before name resolution");
    let _ = writeln!(out, "════════════════════════════════════════════");
    let _ = writeln!(out);

    if rows.is_empty() {
        let _ = writeln!(
            out,
            "** no names in either table — this is a FAULT in `info builtins`, not an \
             empty finding. Both compilers intercept at least the assertion forms; a \
             union of zero means the tables this command reads are empty. **"
        );
        return out;
    }

    let _ = writeln!(
        out,
        "reach: {} name(s) across two tables — bootstrap {bootstrap_total}, self-host {selfhost_total}",
        rows.len()
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  {:<14} {:>9}  {:>9}  VERDICT",
        "NAME", "BOOTSTRAP", "SELF-HOST"
    );
    for row in rows {
        let _ = writeln!(
            out,
            "  {:<14} {:>9}  {:>9}  {}",
            row.name,
            tick(row.bootstrap),
            tick(row.selfhost),
            row.verdict()
        );
    }

    let asymmetric = rows.iter().filter(|r| r.is_asymmetric()).count();
    let undeclared = rows
        .iter()
        .filter(|r| r.is_asymmetric() && r.declared.is_none())
        .count();
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{asymmetric} asymmetry/ies, {undeclared} of them undeclared."
    );
    let _ = writeln!(
        out,
        "An asymmetric name resolves NORMALLY in the compiler that does not intercept it, \
         so any `.tg` definition carrying it becomes that compiler's meaning of the name \
         — silently, and with matching types."
    );
    let _ = writeln!(
        out,
        "See also: `selfhost-conformance --interception-tables` (the gate, which also \
         checks whether a `.tg` definition has appeared under a declared name), \
         `tungsten info def <name> <file> --callers`."
    );
    out
}

/// The single-name report.
#[must_use]
pub fn render_one(name: &str, rows: &[Row], note: Option<String>) -> String {
    let mut out = String::new();
    let Some(row) = rows.iter().find(|r| r.name == name) else {
        let _ = writeln!(
            out,
            "`{name}` is not intercepted by either compiler — it resolves by ordinary \
             name lookup in both."
        );
        let _ = writeln!(
            out,
            "  (that is an answer, not an error: run `tungsten info builtins` with no \
             argument for the names that ARE intercepted.)"
        );
        return out;
    };

    let _ = writeln!(out, "Builtin: {name}");
    let _ = writeln!(out, "{}", "═".repeat(9 + name.len()));
    let _ = writeln!(out);
    let _ = writeln!(out, "  bootstrap: {}", tick(row.bootstrap));
    let _ = writeln!(out, "  self-host: {}", tick(row.selfhost));
    let _ = writeln!(out, "  verdict:   {}", row.verdict());
    if let Some(note) = note {
        let _ = writeln!(out);
        let _ = writeln!(out, "  {note}");
    }
    out
}
