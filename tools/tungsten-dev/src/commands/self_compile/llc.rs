//! Stage 2 of self-compile: fan `llc` out over the emitted `.ll` units.
//!
//! Split out of `self_compile/mod.rs` when ADR 5.8.26b's `ARG_MAX` fix pushed
//! that file past the 400-line threshold.

use super::artifacts::Artifacts;
use super::telemetry::{run_with_rusage, RssLogger};
use super::util::find_files;
use std::process::Command;

/// Build the shell command that fans `llc` out over the unit list.
///
/// The `.ll` paths are **redirected in from `list_path`**, never interpolated
/// into the command string. Embedding them made the pipeline's argv grow with
/// the compiler's own unit count and unit-name length, which exceeded
/// `ARG_MAX` at ~2,109 units once ADR 5.8.26b's `env/module/` split lengthened
/// every unit name under it (`elab__env__module__X` →
/// `elab__env__module__contents__X`) — reported as the opaque
/// `failed to spawn llc pipeline: Argument list too long (os error 7)`. With
/// the list in a file the command is a fixed size, so the limit cannot be
/// reached by growing the compiler.
fn llc_xargs_command(list_path: &str, parallelism: usize, opt_level: &str) -> String {
    format!(
        "xargs -P{parallelism} -I{{}} sh -c \
         'llc -filetype=obj \"$1\" -o \"${{1%.ll}}.o\" {opt_level}' _ {{}} < '{list_path}'"
    )
}

pub(super) fn compile_ir(
    arts: &Artifacts,
    opt_level: &str,
    parallelism: usize,
    rss: &RssLogger,
) -> Result<(), String> {
    let ll_files: Vec<_> =
        find_files(&arts.ir_dir, "ll").map_err(|e| format!("failed to find .ll files: {e}"))?;

    if ll_files.is_empty() {
        return Err("no .ll files found in IR directory".to_string());
    }

    eprintln!("  {} files, {} threads", ll_files.len(), parallelism);

    // The path list goes to a file, not into the command string: see
    // `llc_xargs_command`. `.txt` is invisible to the `find_files(_, "o")`
    // the link stage runs over this same directory.
    let list_path = arts.ir_dir.join("llc-inputs.txt");
    std::fs::write(&list_path, format!("{}\n", ll_files.join("\n"))).map_err(|e| {
        format!(
            "failed to write llc input list {}: {e}",
            list_path.display()
        )
    })?;

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(llc_xargs_command(
        &list_path.to_string_lossy(),
        parallelism,
        opt_level,
    ));

    let output =
        run_with_rusage(&mut cmd).map_err(|e| format!("failed to spawn llc pipeline: {e}"))?;
    rss.log_stage("llc", output.max_rss_kb);

    if !output.status.success() {
        return Err(format!(
            "llc compilation failed (exit {})",
            output.status.code().unwrap_or(-1)
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression ADR 5.8.26b hit: the command must not grow with the unit
    /// list, or a large enough compiler cannot compile itself.
    #[test]
    fn llc_command_size_is_independent_of_unit_count() {
        // The command names the list file, so its length is the same whether
        // that file holds 3 paths or 30,000.
        let cmd = llc_xargs_command("/tmp/ll/llc-inputs.txt", 8, "-O2");
        assert!(
            cmd.len() < 512,
            "command must stay far below ARG_MAX, got {} bytes: {cmd}",
            cmd.len()
        );
    }

    #[test]
    fn llc_command_reads_paths_from_the_list_file() {
        let cmd = llc_xargs_command("/tmp/ll/llc-inputs.txt", 4, "-O0");
        // Redirected in, so the paths never reach argv.
        assert!(
            cmd.contains("< '/tmp/ll/llc-inputs.txt'"),
            "list must be redirected, got: {cmd}"
        );
        assert!(!cmd.contains("echo"), "list must not be echoed into a pipe");
    }

    #[test]
    fn llc_command_threads_parallelism_and_opt_level() {
        let cmd = llc_xargs_command("/tmp/l.txt", 12, "-O2");
        assert!(cmd.contains("-P12"), "parallelism missing: {cmd}");
        assert!(cmd.contains("-O2"), "opt level missing: {cmd}");
        assert!(cmd.contains("llc -filetype=obj"), "llc call missing: {cmd}");
        // `${1%.ll}.o` is what puts each object beside its own .ll.
        assert!(cmd.contains("${1%.ll}.o"), "output mapping missing: {cmd}");
    }
}
