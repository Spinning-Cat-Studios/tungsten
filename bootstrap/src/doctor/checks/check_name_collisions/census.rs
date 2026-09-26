//! The collision census: invert the per-module value tables into a
//! name → defining-modules multimap and classify what comes back (ADR 13.8.26c).
//!
//! Everything here is a pure function over injected data — a [`ModuleInfo`] and
//! a set of `extern "C"` definition sites — so the classification is assertable
//! without parsing a file, let alone elaborating one.

use std::collections::{BTreeMap, HashSet};

use crate::ast::Visibility;
use crate::driver::modules::ModuleInfo;
use crate::elaborate::ModulePath;

/// One place a value name is defined, and the two facts that classify it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionSite {
    /// The module that defines it.
    pub module: ModulePath,
    /// Whether a call site outside that module can name it at all.
    pub visibility: Visibility,
    /// Whether the definition is an `extern "C" fn`, whose Tungsten identifier
    /// *is* its C symbol (ADR 7.8.26b D3) — so a pair is a duplicate link
    /// symbol as well as a shadowed binding.
    pub is_extern_c: bool,
}

/// Why a collision matters, and therefore what its fix is (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CollisionClass {
    /// Two or more `extern "C"` definitions: also a duplicate link symbol, and
    /// no visibility change can fix it — one of them has to be renamed.
    ExternSymbol,
    /// At least one definition is private: every call site of a *losing*
    /// private definition reports E0016, in the loser's file, naming the
    /// winner's module. This is the live class.
    PrivateShadowed,
    /// Every definition is `pub`: latent, and fires the day someone calls the
    /// shadowed one.
    LatentPublic,
}

impl CollisionClass {
    /// The label a report prints for this class.
    pub fn label(self) -> &'static str {
        match self {
            CollisionClass::ExternSymbol => "extern-symbol",
            CollisionClass::PrivateShadowed => "private-shadowed",
            CollisionClass::LatentPublic => "latent-public",
        }
    }

    /// Whether this class is an error *today* rather than a latent one.
    pub fn is_live(self) -> bool {
        match self {
            CollisionClass::ExternSymbol | CollisionClass::PrivateShadowed => true,
            CollisionClass::LatentPublic => false,
        }
    }
}

/// Which definition the flat table currently resolves the name to — the second
/// fact E0016 withholds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Winner {
    /// The module the tree walk registered last, and it is one of these
    /// definitions.
    Definition(ModulePath),
    /// `item_modules` resolves the name to something that is not one of these
    /// value definitions — a same-named type or type alias registered later
    /// took the slot. Out of scope (§3) but named rather than hidden, because
    /// silently printing a wrong winner is worse than reporting the surprise.
    Foreign(ModulePath),
    /// No entry at all. Only reachable when the caller supplies a `ModuleInfo`
    /// whose flat table was not built by the same walk.
    Unrecorded,
}

/// One name bound in more than one module of the reachable tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collision {
    /// The bare name both definitions are keyed on.
    pub name: String,
    /// Every module defining it, ordered by module path.
    pub sites: Vec<DefinitionSite>,
    /// Which one the flat table currently picks.
    pub winner: Winner,
    /// What kind of problem this is.
    pub class: CollisionClass,
}

/// The whole run, reach line included.
///
/// `modules_examined` and `names_considered` sit beside the findings so that
/// `0 collisions` and `0 modules examined` cannot render alike (§2.1) — the
/// failure ADR 11.8.26c shipped and repaired in its own retrospective.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Census {
    /// Modules in the reachable tree.
    pub modules_examined: usize,
    /// Distinct value names bound across them.
    pub names_considered: usize,
    /// Definition sites summed over all names — the multimap's edge count.
    pub definitions_considered: usize,
    /// Every name bound in ≥2 modules, ordered by name.
    pub collisions: Vec<Collision>,
}

/// Whether the `pub use` pass's synthesized copies are subtracted (D1b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReexportHandling {
    /// The shipped behaviour: a re-exported name is one definition reachable by
    /// several paths, which is `reexport-completeness`'s question, not this one.
    Subtract,
    /// Measurement only (AC 1): keep them, so the difference between the two
    /// runs *is* the re-export class.
    Keep,
}

/// Which severity classes a run reports.
///
/// `ValueEnum` is derived here rather than mirrored in the clap layer: a second
/// copy would be one more thing to keep in step, and this ADR's D4 is a whole
/// decision about not making second copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Severity {
    /// Every class, including the latent `pub`/`pub` one.
    All,
    /// Only the classes that are errors today.
    Live,
}

impl Severity {
    /// Whether a finding of this class survives the filter.
    pub fn admits(self, class: CollisionClass) -> bool {
        match self {
            Severity::All => true,
            Severity::Live => class.is_live(),
        }
    }
}

/// Invert `modules[*].values` into the multimap and classify every name bound
/// more than once.
///
/// Reads `modules`, **not** `item_modules`: the flat map's unconditional insert
/// destroys the collision before `ModuleInfo` is finished, so it cannot be its
/// own diagnostic (D1a). It is still the authority on *which* definition wins,
/// which is what [`Winner`] reads it for.
#[allow(clippy::implicit_hasher)] // Reason: callers all use the default hasher
pub fn census(
    info: &ModuleInfo,
    extern_sites: &HashSet<(ModulePath, String)>,
    reexports: ReexportHandling,
) -> Census {
    // Keyed on `segments` rather than on `ModulePath`, which implements no
    // ordering: the inner map both deduplicates a module and fixes the report
    // order, so two runs over the same tree cannot print the sites differently.
    let mut multimap: BTreeMap<&str, BTreeMap<&[String], &ModulePath>> = BTreeMap::new();
    let mut definitions_considered = 0;

    for (module, contents) in &info.modules {
        for name in &contents.values {
            if reexports == ReexportHandling::Subtract
                && contents.reexported_value_sources.contains_key(name)
            {
                continue;
            }
            definitions_considered += 1;
            multimap
                .entry(name.as_str())
                .or_default()
                .insert(module.segments.as_slice(), module);
        }
    }

    let collisions = multimap
        .iter()
        .filter(|(_, modules)| modules.len() > 1)
        .map(|(name, modules)| collision(info, extern_sites, name, modules))
        .collect();

    Census {
        modules_examined: info.modules.len(),
        names_considered: multimap.len(),
        definitions_considered,
        collisions,
    }
}

/// Build one finding from the multimap entry for `name`.
fn collision(
    info: &ModuleInfo,
    extern_sites: &HashSet<(ModulePath, String)>,
    name: &str,
    modules: &BTreeMap<&[String], &ModulePath>,
) -> Collision {
    let sites: Vec<DefinitionSite> = modules
        .values()
        .map(|module| DefinitionSite {
            module: (*module).clone(),
            visibility: info
                .modules
                .get(*module)
                .and_then(|contents| contents.value_visibility.get(name))
                .copied()
                .unwrap_or_default(),
            is_extern_c: extern_sites.contains(&((*module).clone(), name.to_string())),
        })
        .collect();

    Collision {
        class: classify(&sites),
        winner: winner_of(info, name, &sites),
        name: name.to_string(),
        sites,
    }
}

/// Classify a set of definition sites (D3).
///
/// Extern-symbol first: it is the class no visibility change can fix, so a pair
/// that is *both* extern and private-shadowed is reported under the fix that
/// actually applies.
///
/// **Only `Private` triggers the shadowed class — `pub(crate)` does not, and
/// that is a decision rather than an oversight.** A Tungsten program is a single
/// crate, so a `pub(crate)` definition is reachable from every module in it and
/// a losing one raises no E0016: the collision is latent exactly as a `pub`
/// pair's is, and it is reported under the class whose remedy fits. If crates
/// ever become plural, this arm is what has to change first.
fn classify(sites: &[DefinitionSite]) -> CollisionClass {
    if sites.iter().filter(|site| site.is_extern_c).count() > 1 {
        CollisionClass::ExternSymbol
    } else if sites
        .iter()
        .any(|site| site.visibility == Visibility::Private)
    {
        CollisionClass::PrivateShadowed
    } else {
        CollisionClass::LatentPublic
    }
}

/// Read the flat table's current pick and say whether it is one of these
/// definitions.
fn winner_of(info: &ModuleInfo, name: &str, sites: &[DefinitionSite]) -> Winner {
    let Some(recorded) = info.item_modules.get(name) else {
        return Winner::Unrecorded;
    };
    if sites.iter().any(|site| &site.module == recorded) {
        Winner::Definition(recorded.clone())
    } else {
        Winner::Foreign(recorded.clone())
    }
}
