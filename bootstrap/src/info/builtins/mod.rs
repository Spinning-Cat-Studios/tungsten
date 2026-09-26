//! `tungsten info builtins`: which bare names each compiler intercepts (cost 1).
//!
//! ADR 20.8.26c D4. Nothing in any namespace answered this before: a reader who
//! wondered why `info def <name> --callers` said `none` about an obviously-used
//! function had to know the interception table existed to go looking for it, and
//! the whole failure mode is that the answer already looks complete.
//!
//! Reads `tungsten_core::builtins`' constants, which are pinned to the two
//! sources by tests in that crate — so this command answers from anywhere,
//! without a repo checkout, and cannot quietly describe a table that has moved.
//! The *staleness* half (has a `.tg` definition appeared under a name declared
//! to have none?) needs the tree, and is
//! `selfhost-conformance --interception-tables`.

use std::process::ExitCode;

use tungsten_core::builtins::{
    all_intercepted_names, declared_reason, interception_note, is_bootstrap_intercepted,
    is_selfhost_intercepted, BOOTSTRAP_INTERCEPTED, SELFHOST_INTERCEPTED,
};

pub use render::{render_listing, render_one, Row};

mod render;

/// One row per name, in sorted order.
#[must_use]
pub fn rows() -> Vec<Row> {
    all_intercepted_names()
        .into_iter()
        .map(|name| Row {
            name: name.to_string(),
            bootstrap: is_bootstrap_intercepted(name),
            selfhost: is_selfhost_intercepted(name),
            declared: declared_reason(name).map(tungsten_core::builtins::Reason::slug),
        })
        .collect()
}

/// The whole report, as a value.
///
/// Split from [`run`] so the **argument dispatch** is assertable rather than
/// only observable by spawning the binary: `ExitCode` implements no equality,
/// so a `run` that chose its branch inline would leave the no-argument default
/// — the one AC6 is about — testable only through stdout.
#[must_use]
pub fn report(name: Option<&str>) -> String {
    let rows = rows();
    match name {
        Some(name) => render_one(name, &rows, interception_note(name)),
        None => render_listing(
            &rows,
            BOOTSTRAP_INTERCEPTED.len(),
            SELFHOST_INTERCEPTED.len(),
        ),
    }
}

/// Run the command.
pub fn run(name: Option<&str>) -> ExitCode {
    print!("{}", report(name));
    // Read-only either way. A name in neither table is a legitimate question
    // with a "no" answer, not a usage error: the reader asked exactly the right
    // thing and the answer is that nothing intercepts it.
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests;
