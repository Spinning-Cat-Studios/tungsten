//! Keeping `tg_type_tag`'s documented tag table equal to its `match` arms
//! (ADR 13.8.26a D2).
//!
//! The table had been transcribed five times across the repo and three copies
//! had rotted — including the doc comment four lines above the `match` that
//! contradicts it. Correcting copies is what every previous author did; this
//! module is the mechanism that makes the surviving one refutable.
//!
//! The extraction is a pure helper over `&str` so its own adversarial cases can
//! drive it from literals: a test whose only input is the real file has one
//! input and cannot be shown to fail for the right reason. The real file is
//! simply one more input to it.
//!
//! **This parsing exists twice.** `code-health`'s `tag-table-docs` check carries
//! the same `(tag, name)` extraction, because a `tungsten_core` test cannot read
//! `docs/repo-memory/` in the published tree — the two crates sit on opposite
//! sides of the public allowlist, so the logic cannot be shared. That check also
//! reconciles **this** file's doc comment, which is why deleting this module
//! does not return the comment to being unguarded, and which is what
//! cross-checks the two extractors against real input. Change one, read the
//! other.
use std::collections::BTreeSet;

/// Where both halves of the table are anchored. Deliberately not "the first
/// `///` block" or "every `=> N,` in the file": this module lives *in* the
/// file it parses, so an unanchored scan would consume its own fixtures and
/// pass for the wrong reason.
const ANCHOR: &str = "pub extern \"C\" fn tg_type_tag";

/// The catch-all tag: **two** arms return it (`TypeNode::Error` and a `None`
/// handle) against **one** doc entry that names an intent rather than a
/// variant (`99 = unknown / invalid handle`). Comparing names there would
/// fail on correct source, so both sides collapse to one canonical pair —
/// which keeps the tag's *presence* checked in both directions while
/// dropping a name comparison that could not mean anything.
const SENTINEL_TAG: u64 = 99;
const SENTINEL_NAME: &str = "<catch-all>";

/// `(tag, variant-name)` pairs, as a **set**.
///
/// Pairs rather than bare integers because a doc reading `16 = Bogus` passes
/// an integer comparison — the gate would admit exactly the class of error it
/// exists to catch. A set rather than a `Vec` because of [`SENTINEL_TAG`]:
/// under a multiset the correct source fails.
type TagPairs = BTreeSet<(u64, String)>;

struct TagTable {
    documented: TagPairs,
    implemented: TagPairs,
}

fn canonicalize_sentinel((tag, name): (u64, String)) -> (u64, String) {
    if tag == SENTINEL_TAG {
        (tag, SENTINEL_NAME.to_string())
    } else {
        (tag, name)
    }
}

/// One `N = Name` / `N=Name` entry, or `None` for anything else.
///
/// The "anything else" is load-bearing: doc comments here routinely carry ADR
/// ids (`13.5.26l`) and line citations (`predicates.rs:40`), and an extractor
/// scraping loose integers would turn red the day someone cites one — a false
/// positive on the very comment being protected.
fn parse_entry(piece: &str) -> Option<(u64, String)> {
    let (before, name) = piece.split_once('=')?;
    let tag = standalone_trailing_number(before)?;
    let name = name.trim().trim_end_matches('.').trim();
    (!name.is_empty()).then(|| (tag, name.to_string()))
}

/// The digit run that ENDS `text`, provided it stands alone — preceded by
/// whitespace or nothing.
///
/// Splitting the doc block on commas glues the prose ahead of the table onto
/// the first entry (`… rather than a comment. Tags:   0 = Nat`), so the
/// number cannot be required to be the whole left-hand side. The
/// stands-alone rule is what keeps that from also reading a version number:
/// `13.5.26 = …` yields nothing, `Tags:   0 = Nat` yields `0`.
fn standalone_trailing_number(text: &str) -> Option<u64> {
    let trimmed = text.trim_end();
    let len = trimmed
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .count();
    if len == 0 {
        return None;
    }
    let start = trimmed.len() - len;
    match trimmed[..start].chars().next_back() {
        Some(c) if !c.is_whitespace() => None,
        _ => trimmed[start..].parse().ok(),
    }
}

fn documented_pairs(doc: &str) -> TagPairs {
    doc.split(',')
        .filter_map(parse_entry)
        .map(canonicalize_sentinel)
        .collect()
}

/// `Some(TypeNode::Adt(_, _, _))` → `Adt`; a bare `None` arm → `None`.
fn arm_variant(lhs: &str) -> Option<String> {
    if lhs == "None" {
        return Some("None".to_string());
    }
    let rest = lhs.strip_prefix("Some(TypeNode::")?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

fn implemented_pairs(body: &str) -> TagPairs {
    body.lines()
        .filter_map(|line| {
            let (lhs, rhs) = line.split_once("=>")?;
            let tag: u64 = rhs.trim().trim_end_matches(',').trim().parse().ok()?;
            Some((tag, arm_variant(lhs.trim())?))
        })
        .map(canonicalize_sentinel)
        .collect()
}

/// The `///` run immediately preceding the anchor, joined into one string.
///
/// Attribute lines (`#[no_mangle]`) sit between the doc block and the
/// signature and are stepped over; a blank line is **not**, so a distant
/// unrelated doc block can never be adopted.
fn doc_block(before_anchor: &str) -> Option<String> {
    // Drop the anchor's own partial line, so indentation is not read as a
    // non-doc line that ends the run before it starts.
    let head = &before_anchor[..before_anchor.rfind('\n')?];
    let mut collected: Vec<&str> = Vec::new();
    for line in head.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with("#[") {
            continue;
        }
        match trimmed.strip_prefix("///") {
            Some(rest) => collected.push(rest),
            None => break,
        }
    }
    collected.reverse();
    (!collected.is_empty()).then(|| collected.join(" "))
}

/// The brace-balanced body immediately following the anchor.
fn balanced_body(after_anchor: &str) -> Option<&str> {
    let open = after_anchor.find('{')?;
    let mut depth = 0usize;
    for (offset, ch) in after_anchor[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&after_anchor[open + 1..open + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Both halves of the table, or `None` if either could not be read.
///
/// `None` — never two empty sets. A renamed function or a deleted doc block
/// must make the test **fail**; empty-vs-empty is the one way this design
/// could rot into a no-op that reports success forever.
fn tag_table(source: &str) -> Option<TagTable> {
    let at = source.find(ANCHOR)?;
    let documented = documented_pairs(&doc_block(&source[..at])?);
    let implemented = implemented_pairs(balanced_body(&source[at..])?);
    (!documented.is_empty() && !implemented.is_empty()).then_some(TagTable {
        documented,
        implemented,
    })
}

/// Every tag named in `tg_type_tag`'s doc comment is an arm of its `match`,
/// and every arm is named — checked in BOTH directions, so a new variant
/// without a doc entry fails, and so does a doc entry for a deleted variant.
#[test]
fn the_documented_tag_table_matches_the_match_arms() {
    let table = tag_table(include_str!("mod.rs"))
        .expect("tg_type_tag, its doc block and its body are all readable");
    assert_eq!(
        table.documented, table.implemented,
        "tg_type_tag's doc comment and its match arms disagree — correct the \
         comment (ADR 13.8.26a), and remember `docs/repo-memory/codegen-pipeline.md` \
         transcribes the same table under code-health's `tag-table-docs`"
    );
}

/// Sanity on the real file: 18 variant tags (17 = `Int`, ADR 14.9.26c) plus
/// the collapsed sentinel. Without this, a helper that silently dropped every
/// arm would still make the equality above pass — `tag_table`'s emptiness
/// guard rejects *both* sides empty, not one side thinned.
#[test]
fn the_real_table_is_the_size_the_source_says_it_is() {
    let table = tag_table(include_str!("mod.rs")).expect("readable");
    assert_eq!(table.implemented.len(), 19);
    assert!(table.implemented.contains(&(16, "Adt".to_string())));
    assert!(table.implemented.contains(&(17, "Int".to_string())));
    assert!(table
        .implemented
        .contains(&(SENTINEL_TAG, SENTINEL_NAME.to_string())));
}

/// The slice **excludes** its own braces. Pinned directly because `open + 1`
/// and `open * 1` produce the same number whenever `open` is 1 or the arms
/// happen to survive the off-by-one — a mutant that changes the returned text
/// without changing any pair the callers extract.
#[test]
fn the_balanced_body_excludes_its_own_braces() {
    assert_eq!(balanced_body("fn f() { a }").as_deref(), Some(" a "));
    assert_eq!(
        balanced_body("f() { x { y } z }").as_deref(),
        Some(" x { y } z ")
    );
    assert_eq!(balanced_body("{}").as_deref(), Some(""));
}

/// The stands-alone rule, asserted on the function itself: a digit run glued to
/// a version number is not a tag, one preceded by whitespace or nothing is.
#[test]
fn a_trailing_number_counts_only_when_it_stands_alone() {
    assert_eq!(standalone_trailing_number("Tags:   0"), Some(0));
    assert_eq!(standalone_trailing_number("16"), Some(16));
    assert_eq!(standalone_trailing_number("ADR 13.5.26"), None);
    assert_eq!(standalone_trailing_number("predicates.rs:40"), None);
    assert_eq!(standalone_trailing_number("no digits"), None);
    assert_eq!(standalone_trailing_number(""), None);
}

/// A doc block carrying an ADR id and a `file.rs:40` citation extracts
/// cleanly — the false positive that would get this test deleted.
#[test]
fn adr_ids_and_line_citations_contribute_no_tags() {
    let doc = "See ADR 13.5.26l and predicates.rs:40. Tags: 0 = Nat, 1 = Bool";
    assert_eq!(
        documented_pairs(doc),
        [(0, "Nat".to_string()), (1, "Bool".to_string())]
            .into_iter()
            .collect::<TagPairs>()
    );
}

/// A second, decoy `tg_type_tag`-shaped block later in the input is NOT
/// consumed: the anchor takes the first occurrence and nothing else.
#[test]
fn a_later_decoy_block_is_not_consumed() {
    let source = concat!(
        "/// 0 = Nat\n",
        "#[no_mangle]\n",
        "pub extern \"C\" fn tg_type_tag(ty: TypeHandle) -> u64 {\n",
        "    match n {\n",
        "        Some(TypeNode::Nat) => 0,\n",
        "    }\n",
        "}\n",
        "/// 7 = Product\n",
        "pub extern \"C\" fn tg_type_tag(ty: TypeHandle) -> u64 {\n",
        "    match n {\n",
        "        Some(TypeNode::Product(_, _)) => 7,\n",
        "    }\n",
        "}\n",
    );
    let table = tag_table(source).expect("first block reads");
    assert_eq!(
        table.documented,
        [(0, "Nat".to_string())].into_iter().collect::<TagPairs>()
    );
    assert_eq!(table.documented, table.implemented);
}

/// The sentinel's two arms against its one doc entry — the case a multiset
/// comparison fails on correct source, and the case a name comparison would
/// fail on too.
#[test]
fn two_sentinel_arms_reconcile_with_one_doc_entry() {
    let source = concat!(
        "/// 0 = Nat, 99 = unknown / invalid handle\n",
        "pub extern \"C\" fn tg_type_tag(ty: TypeHandle) -> u64 {\n",
        "    match n {\n",
        "        Some(TypeNode::Nat) => 0,\n",
        "        Some(TypeNode::Error) => 99,\n",
        "        None => 99,\n",
        "    }\n",
        "}\n",
    );
    let table = tag_table(source).expect("readable");
    assert_eq!(table.documented, table.implemented);
    assert_eq!(table.implemented.len(), 2);
}

/// A doc entry that keeps the number and lies about the name is caught.
/// This is the case a bare-integer comparison passes.
#[test]
fn a_renamed_doc_entry_is_a_disagreement() {
    let source = concat!(
        "/// 16 = Bogus\n",
        "pub extern \"C\" fn tg_type_tag(ty: TypeHandle) -> u64 {\n",
        "    match n {\n",
        "        Some(TypeNode::Adt(_, _, _)) => 16,\n",
        "    }\n",
        "}\n",
    );
    let table = tag_table(source).expect("readable");
    assert_ne!(table.documented, table.implemented);
}

/// Three ways the input can be unreadable, each of which must yield `None`
/// rather than a vacuously equal pair of empty sets.
#[test]
fn an_unreadable_input_is_none_not_two_empty_sets() {
    let renamed = "pub extern \"C\" fn tg_type_kind() -> u64 { 0 }\n";
    assert!(tag_table(renamed).is_none(), "anchor missing");

    let no_doc = concat!(
        "pub extern \"C\" fn tg_type_tag(ty: TypeHandle) -> u64 {\n",
        "    match n {\n",
        "        Some(TypeNode::Nat) => 0,\n",
        "    }\n",
        "}\n",
    );
    assert!(tag_table(no_doc).is_none(), "doc block deleted");

    let unbalanced = concat!(
        "/// 0 = Nat\n",
        "pub extern \"C\" fn tg_type_tag(ty: TypeHandle) -> u64 {\n",
        "    match n {\n",
        "        Some(TypeNode::Nat) => 0,\n",
    );
    assert!(tag_table(unbalanced).is_none(), "body never closes");

    let no_arms = concat!(
        "/// 0 = Nat\n",
        "pub extern \"C\" fn tg_type_tag(ty: TypeHandle) -> u64 {\n",
        "    unimplemented!()\n",
        "}\n",
    );
    assert!(tag_table(no_arms).is_none(), "no arms is not agreement");
}

/// A blank line ends the doc run: an unrelated block further up the file is
/// never adopted as this function's documentation.
#[test]
fn a_blank_line_ends_the_doc_run() {
    let before = "/// 7 = Product\n\n/// 0 = Nat\n#[no_mangle]\nfoo";
    assert_eq!(doc_block(before).as_deref(), Some(" 0 = Nat"));
}

/// The two arm shapes, and the non-arms that must contribute nothing.
#[test]
fn arm_variants_read_both_shapes_and_reject_the_rest() {
    assert_eq!(
        arm_variant("Some(TypeNode::Adt(_, _, _))").as_deref(),
        Some("Adt")
    );
    assert_eq!(arm_variant("Some(TypeNode::Nat)").as_deref(), Some("Nat"));
    assert_eq!(arm_variant("None").as_deref(), Some("None"));
    assert_eq!(arm_variant("Some(Other::Nat)"), None);
    assert_eq!(arm_variant("Some(TypeNode::)"), None);
}

/// `parse_entry` accepts both spacings and rejects a piece with no number.
#[test]
fn entries_parse_with_and_without_spaces_and_reject_prose() {
    assert_eq!(parse_entry(" 16 = Adt "), Some((16, "Adt".to_string())));
    assert_eq!(parse_entry("16=Adt."), Some((16, "Adt".to_string())));
    assert_eq!(parse_entry("Tags:"), None);
    assert_eq!(parse_entry("see ADR 13.5.26l"), None);
    assert_eq!(parse_entry("x = y"), None);
    assert_eq!(parse_entry("16 = "), None);
}
