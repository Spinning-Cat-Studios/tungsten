//! Rendering for `info codegen symbols --by-function` (ADR 5.8.26b retrospective).
//!
//! Lives here rather than beside `cmd_info_symbols` in `info/commands/mod.rs`
//! for a reason worth stating: this is `#[cfg(feature = "codegen")]` code, and
//! the LLVM-free mutation lane sweeps `bootstrap/**` with
//! `--no-default-features`. A codegen-gated function outside
//! `bootstrap/src/info/codegen/` therefore gets mutants generated for it that
//! **no test in that lane can ever kill**, because the function is not compiled
//! at all — it reports as an under-asserted survivor when the truth is that the
//! sweep cannot see it. `MUTANTS_BOOTSTRAP_EXCLUDES` already excludes this
//! directory; keeping codegen-gated code inside it keeps the signal honest.
//!
//! The decision logic is deliberately NOT here — it is
//! [`crate::info::symbol_names`], which is un-gated and therefore genuinely
//! swept. What remains below is formatting.

use crate::info::symbol_names;

/// `--by-function NAME`: every symbol one source function compiles to.
///
/// The three name-derived symbols always print, because their existence is a
/// property of the emitter's naming rules rather than of this module — a
/// reader summing a profile needs to know to look for `$direct_mt` whether or
/// not this particular build emitted samples into it. Lambdas are joined in
/// from the emitted symbol map, where their presence IS module-specific.
pub(crate) fn print_function_symbol_set(name: &str, symbols: &[tungsten_codegen::SymbolEntry]) {
    println!("Symbol set for `{name}`:");
    println!();
    println!("{:<44} {}", "SYMBOL", "ROLE");
    println!("{}", "─".repeat(100));
    for sym in symbol_names::conventional_symbols(name) {
        println!("{:<44} {}", sym.symbol, sym.role);
    }

    let lambdas: Vec<_> = symbols
        .iter()
        .filter(|e| symbol_names::row_belongs_to(e.source_name.as_deref(), &e.ir_name, name))
        .collect();
    for entry in &lambdas {
        let loc = match (&entry.file, entry.line) {
            (Some(f), Some(l)) => format!("closure body ({f}:{l})"),
            _ => "closure body".to_string(),
        };
        println!("{:<44} {}", entry.ir_name, loc);
    }

    println!();
    println!(
        "{} symbol(s): 3 name-derived + {} lambda(s).",
        3 + lambdas.len(),
        lambdas.len()
    );
    println!(
        "To attribute a `perf` profile to `{name}`, sum self time across ALL of them — \
         `<name>` and `<name>$direct` alone under-count."
    );
}
