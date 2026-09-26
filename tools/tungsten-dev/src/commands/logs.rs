use clap::{Args, Subcommand};
use std::fs;
use std::path::Path;

const LOG_DIR: &str = "/var/log/tungsten";

#[derive(Args)]
pub struct LogsArgs {
    #[command(subcommand)]
    pub command: LogsCommand,
}

#[derive(Subcommand)]
pub enum LogsCommand {
    /// List captured log files
    List,
    /// Print the contents of a specific log file
    Tail {
        /// Log file name (e.g., "self-compile-fast.stderr.log")
        name: String,
    },
    /// Remove all captured log files
    Clean,
}

pub fn run(args: LogsArgs) -> Result<(), String> {
    match args.command {
        LogsCommand::List => list_logs(),
        LogsCommand::Tail { name } => tail_log(&name),
        LogsCommand::Clean => clean_logs(),
    }
}

fn list_logs() -> Result<(), String> {
    let dir = Path::new(LOG_DIR);
    if !dir.exists() {
        println!("(no log directory at {LOG_DIR})");
        return Ok(());
    }

    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("failed to read {LOG_DIR}: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| e.file_name() != ".gitkeep")
        .collect();

    if entries.is_empty() {
        println!("(no log files captured yet)");
        return Ok(());
    }

    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let meta = entry.metadata().ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        println!(
            "  {:>8}  {}",
            format_size(size),
            entry.file_name().to_string_lossy()
        );
    }

    println!("\n{} file(s) in {LOG_DIR}", entries.len());
    Ok(())
}

fn tail_log(name: &str) -> Result<(), String> {
    let path = Path::new(LOG_DIR).join(name);
    if !path.exists() {
        return Err(format!("log file not found: {}", path.display()));
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    print!("{content}");
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

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(512), "512B");
        assert_eq!(format_size(1023), "1023B");
    }

    #[test]
    fn format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.0K");
        assert_eq!(format_size(1536), "1.5K");
    }

    #[test]
    fn format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.0M");
        assert_eq!(format_size(2 * 1024 * 1024 + 512 * 1024), "2.5M");
    }

    #[test]
    fn clean_on_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.stderr.log"), "some error").unwrap();
        fs::write(dir.path().join(".gitkeep"), "").unwrap();

        let mut count = 0;
        for entry in fs::read_dir(dir.path()).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_file() && entry.file_name() != ".gitkeep" {
                fs::remove_file(entry.path()).unwrap();
                count += 1;
            }
        }
        assert_eq!(count, 1);
        // .gitkeep should survive
        assert!(dir.path().join(".gitkeep").exists());
    }
}
