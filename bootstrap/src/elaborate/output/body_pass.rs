//! Pass 2 — body elaboration, shared by all three entry points.
//!
//! Three call sites ran the same six-line loop: `elaborate_file`
//! (single-file), `CollectedElaborator::elaborate` (combined-AST) and
//! `CollectedElaborator::elaborate_with_exports` (the per-module path the
//! driver actually takes). Keeping them separate meant every change to the
//! loop had to be made three times — and ADR 7.8.26d's poison refusal was
//! made twice, leaving `elaborate_file` admitting definitions the other two
//! rejected. The loop lives here now so that cannot recur.

use crate::ast::Item;

use super::CoreDef;
use crate::elaborate::Elaborator;

impl<'a> Elaborator<'a> {
    /// Elaborate every item's body, collecting the definitions that are fit to
    /// emit.
    ///
    /// Errors are recorded rather than returned, so one bad item does not hide
    /// the rest of the file; the caller decides what a non-empty `self.errors`
    /// means. Type definitions produce no `CoreDef`, and a definition whose
    /// type carries poison is refused by [`Elaborator::admit_core_def`]
    /// (ADR 7.8.26d §2.2).
    pub(crate) fn run_body_pass(&mut self, items: &[Item]) -> Vec<CoreDef> {
        let mut defs = Vec::new();
        for item in items {
            match self.elaborate_item(item) {
                Ok(Some(def)) => defs.extend(self.admit_core_def(def)),
                Ok(None) => {} // Type definitions don't produce CoreDefs
                Err(e) => self.record_error(e), // record_error attaches the file path
            }
        }
        defs
    }
}

#[cfg(test)]
mod tests {
    use crate::elaborate::Elaborator;
    use tungsten_core::Context;

    /// Parse a module and run both passes, returning `(def names, error count)`.
    ///
    /// The count is short-circuited collection errors + everything left in
    /// `self.errors` after the body pass. The collection pass defers a fully
    /// poison-compensated error set instead of draining it (ADR 14.8.26g D2),
    /// so this helper counts deferred collection errors too — deliberately
    /// (the D2a audit named this very call site as the one that would
    /// silently start counting them). Summing both routes keeps the count
    /// meaningful whichever way a given error set leaves the pass, and the
    /// exactly-once test below is what keeps that from becoming a
    /// double-count.
    fn body_pass(source: &str) -> (Vec<String>, usize) {
        let (ast, parse_errors) = crate::parse(source);
        assert!(
            parse_errors.is_empty(),
            "fixture must parse: {parse_errors:?}"
        );
        let mut ctx = Context::new();
        let mut elab = Elaborator::new(&mut ctx);
        let short_circuited = elab
            .run_collection_pass(&ast)
            .err()
            .map_or(0, |errors| errors.len());
        let defs = elab.run_body_pass(&ast.items);
        (
            defs.into_iter().map(|d| d.name).collect(),
            short_circuited + elab.errors.len(),
        )
    }

    #[test]
    fn every_value_definition_is_emitted() {
        let (names, errors) = body_pass("fn one() -> Nat { 1 }\nfn two() -> Nat { 2 }");
        assert_eq!(names, vec!["one", "two"]);
        assert_eq!(errors, 0);
    }

    /// Type definitions elaborate but emit no `CoreDef` — the `Ok(None)` arm.
    #[test]
    fn a_type_definition_emits_no_core_def() {
        let (names, errors) = body_pass("type Pair = { a: Nat, b: Nat }\nfn one() -> Nat { 1 }");
        assert_eq!(names, vec!["one"]);
        assert_eq!(errors, 0);
    }

    /// The loop records and continues: a bad item must not cost the good ones.
    #[test]
    fn a_failing_body_does_not_suppress_later_definitions() {
        let (names, errors) =
            body_pass("fn bad() -> Nat { \"not a nat\" }\nfn good() -> Nat { 1 }");
        assert!(names.contains(&"good".to_string()), "got {names:?}");
        assert!(errors > 0, "the failure must still be recorded");
    }

    #[test]
    fn an_empty_module_emits_nothing_and_errors_nothing() {
        assert_eq!(body_pass(""), (vec![], 0));
    }

    /// The deferral stops draining `self.errors` (ADR 14.8.26g D2), so a
    /// collection error must surface exactly once — before dedup, so the
    /// `(span, code)` re-key cannot mask a double-report (D2a). A duplicate
    /// definition records exactly one collection error and elaborates both
    /// bodies cleanly, isolating the count to the collection pass.
    #[test]
    fn a_deferred_collection_error_is_counted_exactly_once() {
        let (names, errors) = body_pass("fn f() -> Nat { 0 }\nfn f() -> Nat { 1 }");
        assert_eq!(
            errors, 1,
            "one recorded collection error must be reported exactly once, pre-dedup"
        );
        assert!(!names.is_empty(), "the surviving body must still elaborate");
    }
}
