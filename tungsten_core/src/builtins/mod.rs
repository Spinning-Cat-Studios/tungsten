//! Which bare names each compiler intercepts before name resolution (ADR 20.8.26c).
//!
//! Both compilers match a simple path's bare name against a fixed table *before*
//! consulting the value environment. The tables are not the same length, and the
//! gap is not cosmetic: a name the bootstrap intercepts and the self-host does
//! not resolves, in the self-host, by ordinary bare-name lookup — so if any `.tg`
//! file happens to define that name, the two compilers elaborate the same call to
//! two different things, silently and with no diagnostic on either side.
//!
//! That is not hypothetical. `substring` was in the bootstrap's table only, and
//! `driver/modules/build/mod.tg` defined a `substring` whose third argument is an
//! **end index** where the builtin's is a **length**. Twelve lexer call sites pass
//! a length. Nothing reported it, in either compiler, for as long as anyone looked.
//!
//! This module is the data behind three consumers:
//!
//! - [`is_bootstrap_intercepted`] — the warning arm on `info def --callers` and
//!   `doctor audit-dead-definitions`, which otherwise report `none` about a
//!   definition the self-host resolves to everywhere;
//! - `tungsten info builtins` — the listing;
//! - `selfhost-conformance --interception-tables` — the gate, which reads the two
//!   *sources* rather than the constants below, so neither can drift unnoticed.
//!
//! The constants are pinned to the sources they describe by [`tests`], via
//! [`scan::bootstrap_table`] and [`scan::selfhost_table`] — the same scanners the
//! gate runs. Editing either table without editing the other therefore fails the
//! build here and the gate there.

pub mod scan;

use std::collections::BTreeSet;

/// Bare names `try_elab_special_application` intercepts
/// (`bootstrap/src/elaborate/exprs/application.rs`), in source order.
pub const BOOTSTRAP_INTERCEPTED: &[&str] = &[
    "ref",
    "get",
    "set",
    "char_at",
    "string_len",
    "substring",
    "expect_type",
    "expect_error",
    "__compare",
    "compare",
    "to_int",
    "from_int",
];

/// Bare names `synth_app` intercepts
/// (`src/compiler/elab/exprs/apply/app/mod.tg`), in source order.
pub const SELFHOST_INTERCEPTED: &[&str] = &[
    "expect_type",
    "expect_error",
    "compare",
    "substring",
    "to_int",
    "from_int",
];

/// Which compiler carries a name the other does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    BootstrapOnly,
    SelfHostOnly,
}

impl Side {
    /// How the report names this side.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Side::BootstrapOnly => "bootstrap only",
            Side::SelfHostOnly => "self-host only",
        }
    }
}

/// Why an asymmetry is tolerated.
///
/// Each variant carries its own staleness condition, checked by [`audit`]. A
/// declaration that has stopped being true is a **failure**, not a silent pass:
/// the whole defect this module exists for was a reason that used to hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Nothing under `src/compiler` defines this bare name, so the self-host
    /// raises `unknown value` rather than resolving to a different meaning.
    /// Stale the moment a `.tg` definition of that name appears.
    NoTgFallback,
    /// A `.tg` definition of this name exists and deliberately agrees with the
    /// builtin. Stale the moment that definition goes away — the entry should
    /// then be [`Reason::NoTgFallback`], which is a stricter promise.
    MatchingTgSemantics,
    /// A second spelling of a name both tables carry; the named twin must be in
    /// both tables for the exemption to hold.
    SpellingAlias(&'static str),
}

impl Reason {
    /// The slug the report prints.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Reason::NoTgFallback => "no-tg-fallback",
            Reason::MatchingTgSemantics => "matching-tg-semantics",
            Reason::SpellingAlias(_) => "spelling-alias",
        }
    }
}

/// Asymmetries this repo tolerates, each with the condition that keeps it true.
///
/// `substring` is deliberately absent: ADR 20.8.26c ported it rather than
/// declaring it, because it was the one name whose `.tg` fallback *disagreed*.
pub const DECLARED_ASYMMETRIES: &[(&str, Reason)] = &[
    ("ref", Reason::NoTgFallback),
    ("get", Reason::NoTgFallback),
    ("set", Reason::NoTgFallback),
    ("char_at", Reason::NoTgFallback),
    ("string_len", Reason::MatchingTgSemantics),
    ("__compare", Reason::SpellingAlias("compare")),
];

/// One name the two tables disagree about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asymmetry {
    pub name: String,
    pub side: Side,
    /// The declared reason, when one applies and still holds.
    pub declared: Option<Reason>,
    /// Why this is a failure, when it is. `None` means the entry is accounted for.
    pub failure: Option<String>,
}

impl Asymmetry {
    /// Whether this entry fails the gate.
    #[must_use]
    pub fn is_failure(&self) -> bool {
        self.failure.is_some()
    }
}

/// The declared reason an asymmetry in `name` is tolerated, if any.
#[must_use]
pub fn declared_reason(name: &str) -> Option<Reason> {
    DECLARED_ASYMMETRIES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, reason)| *reason)
}

/// Every name either table carries, in sorted order.
#[must_use]
pub fn all_intercepted_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = BOOTSTRAP_INTERCEPTED
        .iter()
        .chain(SELFHOST_INTERCEPTED.iter())
        .copied()
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// Is `name` intercepted by the bootstrap before name resolution?
///
/// The predicate behind the `--callers` warning arm. A `.tg` function carrying
/// one of these names is never resolved *by the bootstrap*, so every
/// bootstrap-side reachability answer about it — `--callers`,
/// `audit-dead-definitions` — is `none` and reads as "dead, delete me".
#[must_use]
pub fn is_bootstrap_intercepted(name: &str) -> bool {
    BOOTSTRAP_INTERCEPTED.contains(&name)
}

/// Is `name` intercepted by the self-hosted compiler?
#[must_use]
pub fn is_selfhost_intercepted(name: &str) -> bool {
    SELFHOST_INTERCEPTED.contains(&name)
}

/// The one-line warning `--callers` and `audit-dead-definitions` append.
///
/// Returns `None` for an ordinary name, so the two call sites carry no
/// conditional of their own beyond `if let`.
#[must_use]
pub fn interception_note(name: &str) -> Option<String> {
    if !is_bootstrap_intercepted(name) {
        return None;
    }
    if is_selfhost_intercepted(name) {
        return Some(format!(
            "`{name}` is intercepted as a builtin before name resolution by BOTH compilers, \
             so neither ever resolves it to a `.tg` definition."
        ));
    }
    Some(format!(
        "`{name}` is intercepted as a builtin before name resolution, so the bootstrap never \
         resolves it to a `.tg` definition — but the self-hosted compiler does NOT intercept \
         it and resolves it by bare name, so any `.tg` definition carrying this name is that \
         compiler's meaning of it."
    ))
}

/// Compare two interception tables against the `.tg` definitions in scope.
///
/// Pure over injected data: the gate passes it the two scanned tables and the
/// union of `.tg` definition names, and the tests pass it hand-written ones.
#[must_use]
pub fn audit(
    bootstrap: &[String],
    selfhost: &[String],
    tg_defs: &BTreeSet<String>,
) -> Vec<Asymmetry> {
    let boot: BTreeSet<&str> = bootstrap.iter().map(String::as_str).collect();
    let sh: BTreeSet<&str> = selfhost.iter().map(String::as_str).collect();

    let mut out = Vec::new();
    for name in boot.union(&sh).copied() {
        let side = match (boot.contains(name), sh.contains(name)) {
            (true, false) => Side::BootstrapOnly,
            (false, true) => Side::SelfHostOnly,
            // In both, or in neither — not an asymmetry.
            _ => continue,
        };
        out.push(classify(name, side, &boot, &sh, tg_defs));
    }
    out
}

/// The per-name verdict, split out so each staleness rule is one arm.
fn classify(
    name: &str,
    side: Side,
    boot: &BTreeSet<&str>,
    sh: &BTreeSet<&str>,
    tg_defs: &BTreeSet<String>,
) -> Asymmetry {
    let declared = declared_reason(name);

    let failure = match declared {
        None => Some(format!(
            "undeclared: `{name}` is {} and nothing in DECLARED_ASYMMETRIES accounts for it",
            side.label()
        )),
        Some(Reason::NoTgFallback) if tg_defs.contains(name) => Some(format!(
            "declared `no-tg-fallback`, but `src/compiler` now defines `{name}` — \
             the self-host resolves to it while the bootstrap intercepts"
        )),
        Some(Reason::MatchingTgSemantics) if !tg_defs.contains(name) => Some(format!(
            "declared `matching-tg-semantics`, but `src/compiler` no longer defines `{name}` — \
             re-declare it as `no-tg-fallback`"
        )),
        Some(Reason::SpellingAlias(twin)) if !(boot.contains(twin) && sh.contains(twin)) => Some(
            format!("declared a spelling alias of `{twin}`, which is no longer in both tables"),
        ),
        Some(_) => None,
    };

    Asymmetry {
        name: name.to_string(),
        side,
        declared,
        failure,
    }
}

#[cfg(test)]
mod tests;
