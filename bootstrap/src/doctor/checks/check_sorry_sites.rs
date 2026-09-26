//! `tungsten doctor check sorry-sites <file>` — which definitions carry a
//! proof hole, and who put it there (ADR 18.9.26g).
//!
//! `check` prints `contains sorry` from one bit. This is the census behind the
//! bit: one row per definition whose Core carries a `Sorry`, each hole
//! classified by `Term::sorry_sites` — authored (at `file:line:col`),
//! synthesised (named by the lowering construct that planted it) or
//! unclassified. Cost 3, no codegen. Findings are not failures: exit 0.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use tungsten_core::terms::analysis::{SorryCounts, SorrySite};

use crate::driver::{self, ProjectOutput};
use crate::span::LineIndex;

/// One definition's holes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SorryRow {
    pub name: String,
    /// Each authored hole as `file:line:col`.
    pub authored: Vec<String>,
    /// Each synthesised hole, named by its construct.
    pub synthesised: Vec<&'static str>,
    pub unclassified: usize,
}

/// The whole census: rows in definition order, totals per class, and how many
/// definitions were examined — so an empty census over zero definitions does
/// not read like a clean one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SorryCensus {
    pub examined: usize,
    pub rows: Vec<SorryRow>,
    pub authored: usize,
    pub synthesised: usize,
    pub unclassified: usize,
}

/// Resolves an authored hole's byte span to `file:line:col`.
///
/// `ProjectOutput.defs` is flat and a `TermSpan` carries no file, so the file
/// comes from the codegen unit owning the definition of that name.
struct SiteLocator<'a> {
    source_of: HashMap<&'a str, &'a Path>,
    fallback: Option<&'a Path>,
    project: &'a ProjectOutput,
    line_indexes: HashMap<&'a Path, LineIndex>,
}

impl<'a> SiteLocator<'a> {
    fn new(project: &'a ProjectOutput) -> Self {
        let mut source_of = HashMap::new();
        for unit in &project.codegen_units {
            for def in &unit.defs {
                source_of
                    .entry(def.name.as_str())
                    .or_insert(unit.source_file.as_path());
            }
        }
        Self {
            source_of,
            fallback: project.source_map.main_file(),
            project,
            line_indexes: HashMap::new(),
        }
    }

    fn locate(&mut self, def_name: &str, offset: u32) -> String {
        let Some(file) = self.source_of.get(def_name).copied().or(self.fallback) else {
            return format!("<unknown file>:@{offset}");
        };
        let Some(source) = self.project.source_map.get(file) else {
            return format!("{}:@{offset}", file.display());
        };
        let index = self
            .line_indexes
            .entry(file)
            .or_insert_with(|| LineIndex::new(source));
        let at = index.location(offset);
        format!("{}:{}:{}", file.display(), at.line, at.column)
    }
}

/// Classify every hole in every definition of an elaborated project.
#[must_use]
pub fn census(project: &ProjectOutput) -> SorryCensus {
    let mut locator = SiteLocator::new(project);
    let mut totals = SorryCounts::default();
    let mut rows = Vec::new();
    for def in project.defs.iter().filter(|d| d.term.contains_sorry()) {
        let sites = def.term.sorry_sites();
        totals.add_sites(&sites);
        let mut row = SorryRow {
            name: def.name.clone(),
            authored: Vec::new(),
            synthesised: Vec::new(),
            unclassified: 0,
        };
        for site in sites {
            match site {
                SorrySite::Authored(span) => {
                    row.authored.push(locator.locate(&def.name, span.start));
                }
                SorrySite::Synthesised(construct) => row.synthesised.push(construct.label()),
                SorrySite::Unclassified => row.unclassified += 1,
            }
        }
        rows.push(row);
    }
    SorryCensus {
        examined: project.defs.len(),
        rows,
        authored: totals.authored,
        synthesised: totals.synthesised,
        unclassified: totals.unclassified,
    }
}

/// Human-readable report.
#[must_use]
pub fn render(census: &SorryCensus, file: &str) -> String {
    if census.rows.is_empty() {
        return format!(
            "✓ no definition carries a sorry in {file} ({} definition(s) examined)\n",
            census.examined
        );
    }
    let mut out = format!(
        "⚠ {} of {} definition(s) carry a sorry in {file}:\n\n",
        census.rows.len(),
        census.examined
    );
    for row in &census.rows {
        out.push_str(&format!("  {}\n", row.name));
        for at in &row.authored {
            out.push_str(&format!("    authored      {at}\n"));
        }
        for construct in &row.synthesised {
            out.push_str(&format!("    synthesised   {construct}\n"));
        }
        if row.unclassified > 0 {
            out.push_str(&format!("    unclassified  {}\n", row.unclassified));
        }
    }
    out.push_str(&format!(
        "\ntotals: {} authored, {} synthesised, {} unclassified\n",
        census.authored, census.synthesised, census.unclassified
    ));
    out
}

/// Entry point for `tungsten doctor check sorry-sites <file>`.
pub fn cmd_check_sorry_sites(file: &PathBuf, json: bool, verbose: bool) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, 20, None) {
        Ok(project) => project,
        Err(why) => {
            eprintln!("error: {why}");
            return ExitCode::FAILURE;
        }
    };
    let found = census(&project);
    if json {
        match serde_json::to_string_pretty(&found) {
            Ok(text) => println!("{text}"),
            Err(why) => {
                eprintln!("error: {why}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        print!("{}", render(&found, &file.display().to_string()));
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
#[path = "check_sorry_sites_tests.rs"]
mod tests;
