//! Record `.field` path builders for synthesized comparators
//! (ADR 29.6.26f §2.3 / AC 8).
//!
//! A record value is the right-nested product `(f0, (f1, (f2, …)))` over its
//! fields, so field `i` is `fst(sndⁱ …)` and the last field is `sndⁿ⁻¹`. The
//! comparator walks the fields in declaration order (canonical order), emitting a
//! `Field(name)` segment for each and short-circuiting on the first difference.

use tungsten_core::Term;

use super::{compare_app, equal_term, path_seg, prepend_seg, then_compare};

/// `Field(name)` — a record-field descent segment (the §2.3 `.field`).
#[must_use]
pub fn seg_field(name: &str) -> Term {
    path_seg(0, Term::StringLit(name.to_string()))
}

/// Body of a **record** comparator over `fields = [(field_name, comparator_symbol)]`,
/// emitting source-level `.field` path segments by name, in declaration order.
#[must_use]
pub fn record_comparator_body(fields: &[(String, String)]) -> Term {
    record_body_from(
        fields,
        Term::Var("l".to_string()),
        Term::Var("r".to_string()),
    )
}

fn record_body_from(fields: &[(String, String)], lacc: Term, racc: Term) -> Term {
    match fields {
        [] => equal_term(),
        [(name, sym)] => {
            // Last (or sole) field: the accumulator *is* this field's value.
            prepend_seg(compare_app(sym, lacc, racc), seg_field(name))
        }
        [(name, sym), rest @ ..] => {
            let head = prepend_seg(
                compare_app(
                    sym,
                    Term::Fst(Box::new(lacc.clone())),
                    Term::Fst(Box::new(racc.clone())),
                ),
                seg_field(name),
            );
            let tail = record_body_from(rest, Term::Snd(Box::new(lacc)), Term::Snd(Box::new(racc)));
            then_compare(head, tail)
        }
    }
}
