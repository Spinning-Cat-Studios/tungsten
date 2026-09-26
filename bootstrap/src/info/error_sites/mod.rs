//! `tungsten info error-sites` — where an error code is actually raised.
//!
//! Answers "which code raises E0013?" in one command. The question recurs
//! whenever a diagnostic cascades: ADR 14.8.26g spent a six-copy seeded-corpus
//! measurement discovering that 15 of a variant's 17 reported errors were
//! `E0013` from a single boundary, then found that boundary by hand-grepping
//! for the constructor name — which first meant finding the constructor name.
//! This does all three steps: code → kind → constructor → raise sites.
//!
//! **Cost 1.** No user file is parsed or elaborated; the tool reads the
//! compiler's *own* Rust source. The kind↔code table
//! (`elaborate/error/kind/codes.rs`) is the same oracle `explain`'s catalogue
//! completeness test parses rather than a list typed beside it, so this
//! cannot drift from the real `code()` arms (ADR 8.8.26a).
//!
//! Every step below is a pure function over strings, so the whole report is
//! assertable from in-memory fixtures; only [`run`] touches the filesystem.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[cfg(test)]
mod tests;

/// One place an error kind is constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RaiseSite {
    /// Path as displayed (workspace-relative when possible).
    pub(crate) file: String,
    /// 1-based line number.
    pub(crate) line: usize,
    /// The nearest enclosing `fn` above the site, when one is visible.
    pub(crate) function: Option<String>,
    /// How the kind was built here.
    pub(crate) via: RaiseVia,
}

/// The two ways an error kind reaches a raise site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RaiseVia {
    /// A named constructor, e.g. `ElabError::expected_function(..)`.
    Constructor,
    /// The variant directly, e.g. `ElabErrorKind::ExpectedFunction(..)`.
    Variant,
}

impl RaiseVia {
    fn label(self) -> &'static str {
        match self {
            RaiseVia::Constructor => "constructor",
            RaiseVia::Variant => "variant",
        }
    }
}

/// `(kind name, code)` for every arm of `ElabErrorKind::code()`.
///
/// Parses the flat table rather than trusting a second copy — the arms are
/// one-per-line by contract (see that file's header), and a reformat that
/// broke the shape would empty this, which [`resolve_query`] reports as an
/// unknown code rather than silently finding nothing.
pub(crate) fn parse_kind_codes(codes_src: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for line in codes_src.lines() {
        let Some(rest) = line.trim().strip_prefix("ElabErrorKind::") else {
            continue;
        };
        let Some((lhs, rhs)) = rest.split_once("=>") else {
            continue;
        };
        let kind: String = lhs
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let code: String = rhs
            .trim()
            .trim_matches(|c: char| !c.is_alphanumeric())
            .into();
        if code.len() == 5 && code.starts_with(['E', 'W']) {
            pairs.push((kind, code));
        }
    }
    pairs
}

/// `(constructor fn, kind name)` for every `ElabError` convenience
/// constructor — the `pub fn name(..) -> Self { Self::new(span,
/// ElabErrorKind::Kind..) }` shape in `error/constructors.rs`.
///
/// A constructor whose body spans lines still resolves: the scan remembers
/// the last `pub fn` seen and attaches the next `ElabErrorKind::` it meets.
pub(crate) fn parse_constructors(constructors_src: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut pending_fn: Option<String> = None;
    for line in constructors_src.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("pub fn ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            pending_fn = Some(name);
        }
        if let Some(kind) = kind_mentioned(trimmed) {
            if let Some(name) = pending_fn.take() {
                found.push((name, kind));
            }
        }
    }
    found
}

/// The `ElabErrorKind::Foo` variant named on a line, if any.
fn kind_mentioned(line: &str) -> Option<String> {
    let idx = line.find("ElabErrorKind::")?;
    let rest = &line[idx + "ElabErrorKind::".len()..];
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// The constructors that build `kind`, from a parsed constructor table.
///
/// A named function rather than an inline filter in [`run`] so the selection
/// is assertable: an inverted comparison here would report *other* kinds'
/// constructors, which the exit code cannot see.
pub(crate) fn constructors_for_kind(parsed: &[(String, String)], kind: &str) -> Vec<String> {
    parsed
        .iter()
        .filter(|(_, k)| k == kind)
        .map(|(name, _)| name.clone())
        .collect()
}

/// Resolve a user query — an error code (`E0013`, case-insensitive) or a kind
/// name (`ExpectedFunction`) — to its `(kind, code)` pair.
pub(crate) fn resolve_query(query: &str, pairs: &[(String, String)]) -> Option<(String, String)> {
    let q = query.trim();
    pairs
        .iter()
        .find(|(kind, code)| code.eq_ignore_ascii_case(q) || kind.eq_ignore_ascii_case(q))
        .map(|(kind, code)| (kind.clone(), code.clone()))
}

/// Whether a file is error-module plumbing rather than a raise site.
///
/// The kind↔code table and the constructors themselves *mention* every
/// variant without raising any of them; listing them would bury the handful
/// of real sites. This tool's own source is excluded for the same reason (its
/// docs name a constructor as an example), as are test files — a fixture that
/// constructs an error is not a place the compiler reports one.
pub(crate) fn is_plumbing(path: &str) -> bool {
    let p = path.replace('\\', "/");
    let file = p.rsplit('/').next().unwrap_or(&p);
    p.contains("/elaborate/error/")
        || p.contains("/info/error_sites/")
        || p.contains("/tests/")
        || file.starts_with("test_")
        || file.starts_with("tests_")
        || file.contains("_tests")
        || file == "tests.rs"
}

/// Whether a line *reports* the kind rather than *raising* it.
///
/// A `match` arm that destructures the variant to render a message or pick a
/// hint mentions it exactly as a construction does, and there are more of
/// those than raise sites — `E0013` has four renderers against two real
/// raises. Comment lines are skipped for the same reason: prose naming a
/// constructor is not a call to it.
///
/// `next` is the following line, because an or-pattern spreads one arm over
/// several lines and only the last carries the `=>`:
///
/// ```text
/// ElabErrorKind::TypeMismatch { .. }        <- no `=>` on this line
///     | ElabErrorKind::ExpectedFunction(_)  <- nor this
///     | ElabErrorKind::CannotInferType => HintCategory::TypeMismatch,
/// ```
///
/// So a line is also a pattern when it *starts* an or-branch or the next line
/// continues one.
pub(crate) fn is_not_a_construction(line: &str, next: Option<&str>) -> bool {
    let t = line.trim_start();
    t.starts_with("//")
        || line.contains("=>")
        || t.starts_with('|')
        || next.is_some_and(|n| n.trim_start().starts_with('|'))
}

/// Find every raise site for `kind` across `files` (`(display path, source)`).
///
/// Matches the variant by name and each of `constructors` as
/// `ElabError::<name>(`. Results are ordered by file then line, so the report
/// does not depend on directory-walk order.
pub(crate) fn find_raise_sites(
    kind: &str,
    constructors: &[String],
    files: &[(String, String)],
) -> Vec<RaiseSite> {
    let mut sites = Vec::new();
    for (path, src) in files {
        if is_plumbing(path) {
            continue;
        }
        let lines: Vec<&str> = src.lines().collect();
        let mut enclosing: Option<String> = None;
        for (idx, line) in lines.iter().enumerate() {
            // A `#[cfg(test)]` module inside a production file is still test
            // code; by convention it is the tail of the file, so stop here
            // rather than attribute its fixtures to the compiler.
            if line.trim_start().starts_with("#[cfg(test)]") {
                break;
            }
            if let Some(name) = enclosing_fn_name(line) {
                enclosing = Some(name);
            }
            if is_not_a_construction(line, lines.get(idx + 1).copied()) {
                continue;
            }
            let via = if kind_mentioned(line).as_deref() == Some(kind) {
                Some(RaiseVia::Variant)
            } else if constructors
                .iter()
                .any(|c| line.contains(&format!("ElabError::{c}(")))
            {
                Some(RaiseVia::Constructor)
            } else {
                None
            };
            if let Some(via) = via {
                sites.push(RaiseSite {
                    file: path.clone(),
                    line: idx + 1,
                    function: enclosing.clone(),
                    via,
                });
            }
        }
    }
    sites.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    sites
}

/// The function name declared on a line, for attributing sites to a callee.
fn enclosing_fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("pub(crate) fn ")
        .or_else(|| trimmed.strip_prefix("pub(super) fn "))
        .or_else(|| trimmed.strip_prefix("pub fn "))
        .or_else(|| trimmed.strip_prefix("fn "))?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Render the report a user sees.
pub(crate) fn render_report(
    kind: &str,
    code: &str,
    constructors: &[String],
    sites: &[RaiseSite],
) -> String {
    use std::fmt::Write as _;
    let mut out = format!("{code} — ElabErrorKind::{kind}\n");
    if constructors.is_empty() {
        out.push_str("  constructed directly (no named ElabError constructor)\n");
    } else {
        let names: Vec<String> = constructors
            .iter()
            .map(|c| format!("ElabError::{c}"))
            .collect();
        let _ = writeln!(out, "  constructor(s): {}", names.join(", "));
    }
    if sites.is_empty() {
        out.push_str(
            "\n  no raise sites found — the code is reachable only through \
             plumbing, or the source tree was unavailable\n",
        );
        return out;
    }
    let _ = writeln!(out, "\nraise site(s): {}", sites.len());
    for site in sites {
        let where_ = site
            .function
            .as_deref()
            .map(|f| format!(" in {f}()"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  {}:{}{} [{}]",
            site.file,
            site.line,
            where_,
            site.via.label()
        );
    }
    out
}

/// Every `.rs` file under `root`, as `(display path, source)`.
fn read_rust_sources(root: &Path, base: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            out.extend(read_rust_sources(&path, base));
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(src) = std::fs::read_to_string(&path) {
                let shown = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                out.push((shown, src));
            }
        }
    }
    out
}

/// Where to go when a query does not resolve here.
///
/// A `const` because the claim it makes is about a **different** namespace's
/// listing, so it is checked beside that listing rather than here — no
/// per-subcommand review of `info error-sites` would think to look at what
/// `explain error` withholds, which is how this line went stale (ADR 19.8.26b).
pub(crate) const EXPLAIN_LISTING_HINT: &str =
    "hint: `tungsten explain error` lists every user-facing code";

/// Entry point for `tungsten info error-sites <code>`.
pub(crate) fn run(query: &str) -> ExitCode {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let codes_path = manifest.join("src/elaborate/error/kind/codes.rs");
    let ctors_path = manifest.join("src/elaborate/error/constructors.rs");

    let Ok(codes_src) = std::fs::read_to_string(&codes_path) else {
        eprintln!(
            "error: cannot read the error-code table at {}\n\
             hint: this command reads the compiler's own source, so it needs a \
             development checkout",
            codes_path.display()
        );
        return ExitCode::FAILURE;
    };
    let pairs = parse_kind_codes(&codes_src);
    let Some((kind, code)) = resolve_query(query, &pairs) else {
        eprintln!("error: unknown error code or kind `{query}`\n{EXPLAIN_LISTING_HINT}");
        return ExitCode::FAILURE;
    };

    let ctors_src = std::fs::read_to_string(&ctors_path).unwrap_or_default();
    let constructors = constructors_for_kind(&parse_constructors(&ctors_src), &kind);

    let sources = read_rust_sources(&manifest.join("src"), manifest);
    let sites = find_raise_sites(&kind, &constructors, &sources);
    print!("{}", render_report(&kind, &code, &constructors, &sites));
    ExitCode::SUCCESS
}
