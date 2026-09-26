//! Structural-divergence enrichment for identically-rendered type mismatches
//! (ADR 21.7.26f / D2).
//!
//! An E0010 whose expected and found types *render the same* — the display
//! collapses the difference (a named type vs its stored encoding, an alias vs
//! its expansion, a fresh instantiation vs a stored one) — leaves the reader
//! nothing to act on. Both `Type` trees are in hand at the error site, so this
//! module walks them to the first point they diverge and says where.
//!
//! The enrichment fires *only* when the two renders are byte-equal, so an
//! ordinary mismatch (where the renders already differ) gains nothing: the
//! common case pays no noise.

use tungsten_core::Type;

use crate::driver::output::format_type_for_display;

/// How deep the walk descends before giving up on naming a precise location.
/// Beyond this the path itself stops being readable, and "somewhere very deep"
/// is not a useful thing to print.
const MAX_DIVERGENCE_DEPTH: usize = 12;

/// How deep each side of the divergence is spelled out structurally.
const MAX_RENDER_DEPTH: usize = 4;

/// Hard ceiling on each rendered side, in characters.
///
/// Depth alone does not bound *breadth*: one ADT with a hundred constructors is
/// shallow and still enormous, and the stored encodings this note is most
/// useful on are exactly that shape. A note that dumps a page defeats the
/// readability it exists to provide, so the budget backstops the depth cap.
const MAX_RENDER_CHARS: usize = 160;

/// The first point at which two type trees differ.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Divergence {
    /// Human-readable route from the root to the divergence, outermost first.
    /// Empty when the trees differ at the root.
    pub path: Vec<String>,
    /// The expected side at that point, in structural form.
    pub expected: String,
    /// The found side at that point, in structural form.
    pub found: String,
}

impl Divergence {
    /// Where the divergence is, phrased for a diagnostic note.
    fn location(&self) -> String {
        if self.path.is_empty() {
            "at the top level".to_string()
        } else {
            format!("at {}", self.path.join(" → "))
        }
    }
}

/// The note appended to an E0010 whose two types render identically.
///
/// Returns `None` when the renders differ (nothing is hidden, so nothing to
/// explain) or when the trees are genuinely equal (no divergence to report).
pub(super) fn identical_render_note(expected: &Type, found: &Type) -> Option<String> {
    if format_type_for_display(expected) != format_type_for_display(found) {
        return None;
    }
    let divergence = first_divergence(expected, found)?;
    Some(format!(
        "the two types render identically but differ structurally:\n\
         first divergence {}:\n  \
         expected: {}\n  \
         found:    {}",
        divergence.location(),
        divergence.expected,
        divergence.found,
    ))
}

/// Walk two type trees in parallel and report the first divergence.
///
/// Returns `None` when the trees are structurally equal.
pub(super) fn first_divergence(expected: &Type, found: &Type) -> Option<Divergence> {
    walk(expected, found, &mut Vec::new())
}

/// Descend into matching child positions, recording the route taken.
fn walk(expected: &Type, found: &Type, path: &mut Vec<String>) -> Option<Divergence> {
    if expected == found {
        return None;
    }
    if path.len() >= MAX_DIVERGENCE_DEPTH {
        return Some(divergence_between(expected, found, path));
    }

    match (expected, found) {
        (Type::Arrow(ea, er), Type::Arrow(fa, fr)) => descend(path, "the parameter type", ea, fa)
            .or_else(|| descend(path, "the return type", er, fr)),
        (Type::Product(el, er), Type::Product(fl, fr)) => {
            descend(path, "the left of the product", el, fl)
                .or_else(|| descend(path, "the right of the product", er, fr))
        }
        (Type::Sum(el, er), Type::Sum(fl, fr)) => descend(path, "the left of the sum", el, fl)
            .or_else(|| descend(path, "the right of the sum", er, fr)),
        (Type::Ptr(e), Type::Ptr(f)) => descend(path, "the pointee", e, f),
        (Type::Ref(e), Type::Ref(f)) => descend(path, "the referent", e, f),
        (Type::Mu(ev, eb), Type::Mu(fv, fb)) => {
            binder_walk(BinderPair::new("μ", ev, eb, fv, fb), path, expected, found)
        }
        (Type::Forall(ev, eb), Type::Forall(fv, fb)) => {
            binder_walk(BinderPair::new("∀", ev, eb, fv, fb), path, expected, found)
        }
        (Type::App(en, ea), Type::App(fn_, fa)) if en == fn_ && ea.len() == fa.len() => {
            walk_args(&format!("`{en}`"), ea, fa, path)
        }
        (Type::Adt(en, ea, ev), Type::Adt(fn_, fa, fv))
            if en == fn_ && ea.len() == fa.len() && ev.len() == fv.len() =>
        {
            walk_args(&format!("`{en}`"), ea, fa, path).or_else(|| walk_variants(ev, fv, path))
        }
        // Different shapes at this position: this is the divergence.
        _ => Some(divergence_between(expected, found, path)),
    }
}

/// A pair of matching binder types (μ or ∀) being compared.
struct BinderPair<'a> {
    symbol: &'a str,
    expected_var: &'a str,
    expected_body: &'a Type,
    found_var: &'a str,
    found_body: &'a Type,
}

impl<'a> BinderPair<'a> {
    fn new(
        symbol: &'a str,
        expected_var: &'a str,
        expected_body: &'a Type,
        found_var: &'a str,
        found_body: &'a Type,
    ) -> Self {
        BinderPair {
            symbol,
            expected_var,
            expected_body,
            found_var,
            found_body,
        }
    }
}

/// Compare a binder: a differing bound variable is itself the divergence,
/// otherwise descend into the body.
fn binder_walk(
    pair: BinderPair<'_>,
    path: &mut Vec<String>,
    expected: &Type,
    found: &Type,
) -> Option<Divergence> {
    if pair.expected_var != pair.found_var {
        return Some(divergence_between(expected, found, path));
    }
    descend(
        path,
        &format!("the {} body", pair.symbol),
        pair.expected_body,
        pair.found_body,
    )
}

/// Walk parallel type-argument lists, naming the argument position.
fn walk_args(
    owner: &str,
    expected: &[Type],
    found: &[Type],
    path: &mut Vec<String>,
) -> Option<Divergence> {
    expected
        .iter()
        .zip(found.iter())
        .enumerate()
        .find_map(|(i, (e, f))| {
            descend(path, &format!("type argument #{} of {owner}", i + 1), e, f)
        })
}

/// Walk parallel ADT variant lists, naming the constructor.
fn walk_variants(
    expected: &[(String, Type)],
    found: &[(String, Type)],
    path: &mut Vec<String>,
) -> Option<Divergence> {
    expected
        .iter()
        .zip(found.iter())
        .find_map(|((en, et), (fn_, ft))| {
            if en != fn_ {
                let mut owned = path.clone();
                owned.push("a constructor name".to_string());
                return Some(Divergence {
                    path: owned,
                    expected: en.clone(),
                    found: fn_.clone(),
                });
            }
            descend(path, &format!("the payload of `{en}`"), et, ft)
        })
}

/// Push a path segment, walk the child, and pop — so the path always describes
/// the route to whatever divergence is returned.
fn descend(
    path: &mut Vec<String>,
    segment: &str,
    expected: &Type,
    found: &Type,
) -> Option<Divergence> {
    path.push(segment.to_string());
    let result = walk(expected, found, path);
    path.pop();
    result
}

/// Describe the divergence between two nodes at the current position.
fn divergence_between(expected: &Type, found: &Type, path: &[String]) -> Divergence {
    Divergence {
        path: path.to_vec(),
        expected: render_side(expected),
        found: render_side(found),
    }
}

/// Render one side of a divergence, bounded in both depth and length.
fn render_side(ty: &Type) -> String {
    truncate_to_budget(
        ty.display_detailed_to_depth(MAX_RENDER_DEPTH),
        MAX_RENDER_CHARS,
    )
}

/// Cut `text` to `budget` characters, marking the elision.
///
/// Counts characters rather than bytes: type spellings carry multi-byte
/// characters (`α_List`, `μ`, the `…` this very function appends), and a byte
/// cut would land mid-character.
fn truncate_to_budget(mut text: String, budget: usize) -> String {
    match text.char_indices().nth(budget) {
        None => text,
        Some((cut, _)) => {
            text.truncate(cut);
            text.push('…');
            text
        }
    }
}

#[cfg(test)]
#[path = "tests_structural_divergence.rs"]
mod tests;

#[cfg(test)]
#[path = "tests_structural_divergence_note.rs"]
mod tests_note;
