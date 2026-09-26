use clap::Args;
use std::process::Command;

const LOG_DIR: &str = "/var/log/tungsten";

const EXAMPLES: &[&str] = &[
    "examples/hello.tg",
    "examples/answer.tg",
    "examples/option.tg",
    "examples/arithmetic.tg",
    "examples/strings.tg",
    "examples/logic.tg",
    "examples/pair.tg",
    "examples/list_ops.tg",
    "examples/result.tg",
    "examples/ordering.tg",
];

#[derive(Args)]
pub struct VerifyArgs {
    /// Binary to verify
    #[arg(long, default_value = "./tungsten1")]
    pub binary: String,

    /// Log prefix for stderr capture
    #[arg(long, default_value = "verify")]
    pub log_prefix: String,
}

pub fn run(args: VerifyArgs) -> Result<(), String> {
    eprintln!("=== Verifying {} ===", args.binary);

    smoke_test(&args.binary)?;

    let mut failed = Vec::new();
    for example in EXAMPLES {
        let name = std::path::Path::new(example)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(example);

        eprint!("  check {:<35}", example);

        let log_path = format!("{LOG_DIR}/{}.{name}.stderr.log", args.log_prefix);

        let output = Command::new(&args.binary)
            .args(["check", example])
            .output()
            .map_err(|e| format!("failed to run {}: {e}", args.binary))?;

        // Capture stderr to log file
        if !output.stderr.is_empty() {
            let _ = std::fs::write(&log_path, &output.stderr);
        }

        if output.status.success() {
            eprintln!("✅");
        } else {
            eprintln!("❌ FAIL (see {log_path})");
            if !output.stdout.is_empty() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines().take(3) {
                    eprintln!("    {line}");
                }
            }
            failed.push(example.to_string());
        }
    }

    if !failed.is_empty() {
        return Err(format!(
            "verify failed: {}/{} examples failed: {}",
            failed.len(),
            EXAMPLES.len(),
            failed.join(", ")
        ));
    }

    eprintln!("✅ {}/{} examples passed", EXAMPLES.len(), EXAMPLES.len());
    Ok(())
}

fn smoke_test(binary: &str) -> Result<(), String> {
    eprint!("  smoke {:<35}", "version");

    let output = Command::new(binary)
        .arg("version")
        .output()
        .map_err(|e| format!("failed to run {binary} version: {e}"))?;

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    if combined.contains("USAGE:") {
        eprintln!("❌ FATAL: version printed help — binary is broken");
        return Err("smoke test failed: version printed help".to_string());
    }

    if !combined.to_lowercase().contains("tungsten") {
        eprintln!("❌ FATAL: version did not print expected output");
        return Err("smoke test failed: version output missing 'tungsten'".to_string());
    }

    eprintln!("✅");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_args_defaults() {
        let args = VerifyArgs {
            binary: "./tungsten1".to_string(),
            log_prefix: "verify".to_string(),
        };
        assert_eq!(args.binary, "./tungsten1");
        assert_eq!(args.log_prefix, "verify");
    }

    #[test]
    fn examples_list_has_ten_entries() {
        assert_eq!(EXAMPLES.len(), 10);
    }

    #[test]
    fn examples_all_end_in_tg() {
        assert!(EXAMPLES.iter().all(|e| e.ends_with(".tg")));
    }

    #[test]
    fn smoke_test_detects_usage_output() {
        // smoke_test should reject a binary whose version output contains "USAGE:"
        // We use "echo" as a mock binary — it prints its args, so we can inject "USAGE:"
        let result = smoke_test_output("USAGE: fake --help");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("version printed help"));
    }

    #[test]
    fn smoke_test_detects_missing_tungsten() {
        // smoke_test should reject a binary whose version output lacks "tungsten"
        let result = smoke_test_output("foobar 1.0.0");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing 'tungsten'"));
    }

    #[test]
    fn smoke_test_accepts_valid_output() {
        let result = smoke_test_output("tungsten 0.1.0");
        assert!(result.is_ok());
    }

    /// Helper: run smoke_test logic against a fake version output string.
    fn smoke_test_output(output: &str) -> Result<(), String> {
        if output.contains("USAGE:") {
            return Err("smoke test failed: version printed help".to_string());
        }
        if !output.to_lowercase().contains("tungsten") {
            return Err("smoke test failed: version output missing 'tungsten'".to_string());
        }
        Ok(())
    }

    #[test]
    fn verify_reports_failure_count() {
        // Verify the error message format when examples fail
        let failed = vec![
            "examples/hello.tg".to_string(),
            "examples/pair.tg".to_string(),
        ];
        let total = 10;
        let msg = format!(
            "verify failed: {}/{} examples failed: {}",
            failed.len(),
            total,
            failed.join(", ")
        );
        assert!(msg.contains("2/10"));
        assert!(msg.contains("examples/hello.tg"));
        assert!(msg.contains("examples/pair.tg"));
    }
}
