//! Rendering for `doctor check module name-collisions` — human report and JSON.
//!
//! The report shape is ADR 12.7.26b's, deliberately: every candidate listed,
//! the current winner marked. A reader who has seen `extern-map-ambiguity` can
//! read this without learning a second vocabulary (§1.4).

use crate::ast::Visibility;

use super::census::{Census, Collision, CollisionClass, DefinitionSite, Severity, Winner};

/// The reach line: what the run actually looked at.
///
/// Printed on every run, findings or not. `0 collisions` and `0 modules
/// examined` are different outcomes and must not render alike (§2.1).
pub fn reach_line(census: &Census) -> String {
    format!(
        "  examined {} module(s), {} distinct value name(s) across {} definition site(s)\n",
        census.modules_examined, census.names_considered, census.definitions_considered
    )
}

/// The whole human report, as a value so every branch is assertable.
pub fn render_human(census: &Census, reported: &[&Collision], severity: Severity) -> String {
    let mut out = String::new();
    if reported.is_empty() {
        out.push_str(&format!(
            "✓ No name collisions ({}).\n",
            scope_label(severity)
        ));
    } else {
        out.push_str(&format!(
            "! {} name collision(s) ({}):\n",
            reported.len(),
            scope_label(severity)
        ));
        for collision in reported {
            out.push_str(&render_collision(collision));
        }
    }
    out.push_str(&reach_line(census));
    if !reported.is_empty() {
        out.push_str(ADVISORY);
    }
    out
}

/// Why the exit code is 0 anyway (D3). Said on findings, so nobody reads the
/// zero as "and therefore fine".
const ADVISORY: &str =
    "  advisory: exit is 0 even with findings — read `--json` to gate on them (ADR 13.8.26c D3)\n";

/// What `--severity` selected, in words.
fn scope_label(severity: Severity) -> &'static str {
    match severity {
        Severity::All => "all classes",
        Severity::Live => "live classes only: extern-symbol, private-shadowed",
    }
}

/// One finding: the name, its class, every defining module, and the winner.
fn render_collision(collision: &Collision) -> String {
    let mut out = format!("\n  `{}` [{}]\n", collision.name, collision.class.label());
    for site in &collision.sites {
        out.push_str(&format!(
            "    {}{}{}\n",
            site.module,
            site_suffix(site),
            winner_marker(&collision.winner, site)
        ));
    }
    if let Winner::Foreign(module) = &collision.winner {
        out.push_str(&format!(
            "    (the flat table resolves `{}` to `{}`, which defines no value of \
             that name — a same-named type registered later)\n",
            collision.name, module
        ));
    }
    out.push_str(&format!("    → {}\n", remedy(collision.class)));
    out
}

/// The visibility and extern-ness of one site, in the compact form.
fn site_suffix(site: &DefinitionSite) -> String {
    let visibility = match site.visibility {
        Visibility::Private => "private",
        Visibility::Crate => "pub(crate)",
        Visibility::Public => "pub",
    };
    if site.is_extern_c {
        format!("  ({visibility}, extern \"C\")")
    } else {
        format!("  ({visibility})")
    }
}

/// The marker on the site the flat table currently picks.
fn winner_marker(winner: &Winner, site: &DefinitionSite) -> &'static str {
    match winner {
        Winner::Definition(module) if module == &site.module => "   ← registered last, wins",
        _ => "",
    }
}

/// What to do about a finding of this class.
fn remedy(class: CollisionClass) -> &'static str {
    match class {
        CollisionClass::ExternSymbol => {
            "the Tungsten identifier IS the C symbol, so no visibility change helps — rename one"
        }
        CollisionClass::PrivateShadowed => {
            "every call site of the losing definition reports E0016 in its own file, \
             naming the winner's module — rename one"
        }
        CollisionClass::LatentPublic => {
            "latent: calls resolve to the winner today, and break the day the loser is called"
        }
    }
}

/// The machine-readable form. Same fields, so a gate can be built on it without
/// parsing prose (D3).
pub fn render_json(census: &Census, reported: &[&Collision], severity: Severity) -> String {
    let findings: Vec<serde_json::Value> = reported
        .iter()
        .map(|collision| {
            serde_json::json!({
                "name": collision.name,
                "class": collision.class.label(),
                "winner": winner_json(&collision.winner),
                "sites": collision.sites.iter().map(site_json).collect::<Vec<_>>(),
            })
        })
        .collect();
    let value = serde_json::json!({
        "severity": match severity { Severity::All => "all", Severity::Live => "live" },
        "modules_examined": census.modules_examined,
        "names_considered": census.names_considered,
        "definitions_considered": census.definitions_considered,
        "collision_count": reported.len(),
        "collisions": findings,
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&value).unwrap_or_default()
    )
}

/// One site as JSON.
fn site_json(site: &DefinitionSite) -> serde_json::Value {
    serde_json::json!({
        "module": site.module.to_string(),
        "visibility": match site.visibility {
            Visibility::Private => "private",
            Visibility::Crate => "crate",
            Visibility::Public => "pub",
        },
        "extern_c": site.is_extern_c,
    })
}

/// The winner as JSON: the module, plus whether it is one of the sites.
fn winner_json(winner: &Winner) -> serde_json::Value {
    match winner {
        Winner::Definition(module) => serde_json::json!({
            "module": module.to_string(), "kind": "definition",
        }),
        Winner::Foreign(module) => serde_json::json!({
            "module": module.to_string(), "kind": "foreign",
        }),
        Winner::Unrecorded => serde_json::json!({ "kind": "unrecorded" }),
    }
}
