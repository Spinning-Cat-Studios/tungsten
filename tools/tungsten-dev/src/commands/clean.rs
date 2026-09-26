use clap::{Args, Subcommand};
use std::fs;
use std::path::Path;

const IR_DIR: &str = "/tmp/tungsten1_ll";
const WORKSPACE_IR_DIR: &str = "tungsten1_ll";
const TARGET_LL_DIR: &str = "target/ll";
const LOG_DIR: &str = "/var/log/tungsten";

#[derive(Args)]
pub struct CleanArgs {
    #[command(subcommand)]
    pub command: CleanCommand,
}

#[derive(Subcommand)]
pub enum CleanCommand {
    /// Remove IR output directories (/tmp/tungsten1_ll/ and tungsten1_ll/)
    Ir,
    /// Remove captured log files (alias for `logs clean`)
    Logs,
    /// Remove all: IR directories, logs, and target/ll/
    All,
}

pub fn run(args: CleanArgs) -> Result<(), String> {
    match args.command {
        CleanCommand::Ir => clean_ir(),
        CleanCommand::Logs => clean_logs(),
        CleanCommand::All => {
            clean_ir()?;
            clean_logs()?;
            clean_target_ll()
        }
    }
}

fn clean_ir() -> Result<(), String> {
    let mut removed = Vec::new();

    for dir in [IR_DIR, WORKSPACE_IR_DIR] {
        let path = Path::new(dir);
        if path.exists() {
            fs::remove_dir_all(path).map_err(|e| format!("failed to remove {dir}: {e}"))?;
            removed.push(dir);
        }
    }

    if removed.is_empty() {
        println!("(no IR directories to clean)");
    } else {
        println!("Removed: {}", removed.join(", "));
    }
    Ok(())
}

fn clean_logs() -> Result<(), String> {
    let dir = Path::new(LOG_DIR);
    if !dir.exists() {
        println!("(no log directory)");
        return Ok(());
    }

    let mut count = 0;
    for entry in fs::read_dir(dir).map_err(|e| format!("read dir: {e}"))? {
        let entry = entry.map_err(|e| format!("entry: {e}"))?;
        if entry.path().is_file() && entry.file_name() != ".gitkeep" {
            fs::remove_file(entry.path())
                .map_err(|e| format!("remove {}: {e}", entry.path().display()))?;
            count += 1;
        }
    }

    println!("Removed {count} log file(s)");
    Ok(())
}

fn clean_target_ll() -> Result<(), String> {
    let path = Path::new(TARGET_LL_DIR);
    if path.exists() {
        fs::remove_dir_all(path).map_err(|e| format!("failed to remove {TARGET_LL_DIR}: {e}"))?;
        println!("Removed {TARGET_LL_DIR}/");
    }
    Ok(())
}
