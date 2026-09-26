//! `--update` mode for the golden runner. Split out of `main.rs` (ADR 16.7.26b
//! file-size paydown). Regenerates the `.expected` snapshot for a test from the
//! current compiler output (or the compiled-then-run output for `compile`).

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::compiler::{read_args_file, run_compiler};
use crate::{Category, Cli};

pub(crate) fn update_test(cli: &Cli, category: &Category, tg: &Path, expected: &Path) {
    if matches!(category, Category::Compile) {
        update_compile_test(cli, tg, expected);
        return;
    }

    let extra_args = read_args_file(tg);
    let actual = run_compiler(&cli.compiler, category.tungsten_cmd(), tg, &extra_args);
    let content = format!("{}\n", actual);

    if let Err(e) = fs::write(expected, &content) {
        println!(
            "\x1b[31mFailed to write\x1b[0m {} ({e})",
            expected.display()
        );
        return;
    }
    println!("\x1b[32mUpdated\x1b[0m {}", expected.display());
}

fn update_compile_test(cli: &Cli, tg: &Path, expected: &Path) {
    let compile_output = Command::new(&cli.compiler)
        .args(["compile", &tg.to_string_lossy()])
        .output();

    let ok = match &compile_output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    };

    if !ok {
        println!("\x1b[31mFailed to compile\x1b[0m {}", tg.display());
        return;
    }

    let binary = tg.with_extension("");
    if !binary.exists() {
        println!("\x1b[31mNo binary produced\x1b[0m for {}", tg.display());
        return;
    }

    let output = Command::new(&binary).output();
    let _ = fs::remove_file(&binary);

    let actual = match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim_end().to_string(),
        Err(e) => {
            println!("\x1b[31mFailed to run\x1b[0m {} ({e})", tg.display());
            return;
        }
    };
    let content = format!("{}\n", actual);

    if let Err(e) = fs::write(expected, &content) {
        println!(
            "\x1b[31mFailed to write\x1b[0m {} ({e})",
            expected.display()
        );
        return;
    }
    println!("\x1b[32mUpdated\x1b[0m {}", expected.display());
}
