use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "tungsten-dev")]
#[command(about = "Internal development tool for Tungsten devcontainer workflows")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Orchestrate self-compile pipeline (IR → llc → link)
    SelfCompile(commands::self_compile::SelfCompileArgs),
    /// Rebuild the bootstrap compiler if Cargo says it is stale — the same
    /// canonical build `self-compile` runs first (ADR 24.9.26a)
    EnsureBootstrap,
    /// Manage captured log files in /var/log/tungsten
    Logs(commands::logs::LogsArgs),
    /// Clean build artifacts
    Clean(commands::clean::CleanArgs),
    /// Build with tracing and capture a Chrome trace (ADR 10.5.26j)
    Profile(commands::profile::ProfileArgs),
    /// Self-compiled heap profile: alloc-profiled tungsten1 check + RSS sampling +
    /// per-module delta summary (ADR 2.7.26a §3.4 / 11.7.26a)
    SelfcompiledProfile(commands::selfcompiled_profile::SelfcompiledProfileArgs),
    /// Verify a self-compiled binary against example programs
    Verify(commands::verify::VerifyArgs),
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::SelfCompile(args) => commands::self_compile::run(args),
        Commands::EnsureBootstrap => commands::self_compile::ensure_bootstrap(),
        Commands::Logs(args) => commands::logs::run(args),
        Commands::Clean(args) => commands::clean::run(args),
        Commands::Profile(args) => commands::profile::run(args),
        Commands::SelfcompiledProfile(args) => commands::selfcompiled_profile::run(args),
        Commands::Verify(args) => commands::verify::run(args),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_parses_without_panic() {
        // Verifies the clap derive macros produce a valid CLI structure
        Cli::command().debug_assert();
    }
}
