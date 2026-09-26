use clap::{Parser, ValueEnum};
use std::path::PathBuf;

mod compare;
mod compiler;
mod discover;
mod preflight;
mod run;
mod update;

use run::{clean_cache, run_category, summary_line, CategoryTally};

/// Golden test runner for the Tungsten compiler.
///
/// Compares compiler output against `.expected` snapshot files.
/// Strips ANSI color codes and normalizes paths before comparison.
///
/// The discovery (`discover`), compiler invocation (`compiler`), comparison
/// (`compare`), execution (`run`), and `--update` (`update`) logic live in
/// sibling modules (ADR 16.7.26b file-size paydown).
#[derive(Parser)]
#[command(
    name = "golden",
    about = "Golden test runner for the Tungsten compiler"
)]
pub(crate) struct Cli {
    /// Test category to run (omit to run all)
    #[arg(value_enum)]
    category: Option<Category>,

    /// Regenerate .expected files from current compiler output
    #[arg(long)]
    update: bool,

    /// Path to the tungsten compiler binary
    #[arg(long, default_value = "./target/release/tungsten")]
    compiler: PathBuf,

    /// Path to the golden tests directory
    #[arg(long, default_value = "tests/golden")]
    dir: PathBuf,

    /// Skip cache cleaning between categories
    #[arg(long)]
    no_clean: bool,
}

#[derive(Clone, ValueEnum)]
pub(crate) enum Category {
    Check,
    Run,
    Error,
    Compile,
    Test,
}

impl Category {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Category::Check => "check",
            Category::Run => "run",
            Category::Error => "error",
            Category::Compile => "compile",
            Category::Test => "test",
        }
    }

    pub(crate) fn tungsten_cmd(&self) -> &str {
        match self {
            Category::Check | Category::Error => "check",
            Category::Run => "run",
            Category::Compile => "compile",
            Category::Test => "test",
        }
    }

    fn all() -> Vec<Category> {
        vec![
            Category::Check,
            Category::Run,
            Category::Error,
            Category::Compile,
            Category::Test,
        ]
    }
}

fn main() {
    let cli = Cli::parse();

    if !cli.compiler.exists() {
        eprintln!(
            "Compiler not found at {}. Build with: cargo build --release -p tungsten_bootstrap",
            cli.compiler.display()
        );
        std::process::exit(1);
    }

    let categories = match &cli.category {
        Some(c) => vec![c.clone()],
        None => Category::all(),
    };

    println!("Golden Test Runner");
    println!("==================");
    println!();

    // Probe the binary once, before any category runs (ADR 21.7.26f / D3).
    // An unrunnable binary would otherwise false-red every test in every
    // category with a per-test "failed to run compiler" diff.
    let preflight = preflight::probe(&cli.compiler);
    println!("{}", preflight.banner(&cli.compiler));
    println!();
    if preflight.is_fatal() {
        std::process::exit(1);
    }
    let codegen_available = preflight.codegen_available();

    let mut total = CategoryTally::default();

    for category in &categories {
        if !cli.no_clean {
            clean_cache(&cli.compiler);
        }
        total.add(run_category(&cli, category, codegen_available));
    }

    if categories.len() > 1 {
        println!("==================");
        print!("Total: {}", summary_line(&total).trim_start());
    }

    if total.is_failure() {
        std::process::exit(1);
    }
}
