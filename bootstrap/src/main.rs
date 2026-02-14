// Clippy lint policy: inherit suppression from lib.rs for the binary crate.
#![allow(
    clippy::similar_names,
    clippy::ptr_arg,
    clippy::items_after_statements,
    clippy::match_same_arms
)]

//! Tungsten Bootstrap Compiler — CLI Driver
//!
//! This is the command-line interface for the Tungsten bootstrap compiler.
//! It provides commands for type-checking, running, compiling, and interacting with
//! Tungsten source files.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use tungsten_bootstrap::driver::{self, Mode, PipelineResult};

#[derive(Parser)]
#[command(name = "tungsten")]
#[command(author, version, about = "The Tungsten proof language compiler")]
#[command(
    long_about = "Tungsten is a proof language that combines programming and theorem proving.\n\n\
                  This is the bootstrap compiler, written in Rust. Once Tungsten is self-hosting,\n\
                  it will be replaced by a compiler written in Tungsten itself."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Run a file directly (shorthand for `tungsten run <FILE>`)
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// Show verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Maximum number of errors to display (0 = no limit)
    #[arg(long, global = true, default_value = "20")]
    max_errors: usize,
}

#[derive(Subcommand)]
enum Commands {
    /// Type-check a file without running
    Check {
        /// The source file to check
        file: PathBuf,

        /// Disable build cache (force full recompilation)
        #[arg(long)]
        no_cache: bool,
    },

    /// Type-check and evaluate a file
    Run {
        /// The source file to run
        file: PathBuf,

        /// Disable build cache (force full recompilation)
        #[arg(long)]
        no_cache: bool,
    },

    /// Compile a file to a native executable
    #[cfg(feature = "codegen")]
    Compile {
        /// The source file to compile
        file: PathBuf,

        /// Output file path (defaults to input name without extension)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Emit LLVM IR instead of executable
        #[arg(long)]
        emit_llvm: bool,
    },

    /// Evaluate an expression
    Eval {
        /// The expression to evaluate
        expr: String,
    },

    /// Start interactive REPL
    Repl,

    /// Clear the build cache
    Clean,

    /// Manage the build cache
    #[command(subcommand)]
    Cache(CacheCommands),
}

#[derive(Subcommand)]
enum CacheCommands {
    /// Show cache statistics
    Stats {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Prune cache to target size (removes least recently used entries)
    Prune {
        /// Target size in MB (defaults to configured `max_size_mb`)
        #[arg(long)]
        target_mb: Option<u64>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Handle direct file argument: `tungsten hello.tg` → `tungsten run hello.tg`
    if let Some(file) = cli.file {
        return cmd_run(&file, cli.verbose, false, cli.max_errors);
    }

    match cli.command {
        Some(Commands::Check { file, no_cache }) => {
            cmd_check(&file, cli.verbose, no_cache, cli.max_errors)
        }
        Some(Commands::Run { file, no_cache }) => {
            cmd_run(&file, cli.verbose, no_cache, cli.max_errors)
        }
        #[cfg(feature = "codegen")]
        Some(Commands::Compile {
            file,
            output,
            emit_llvm,
        }) => cmd_compile(
            &file,
            output.as_deref(),
            emit_llvm,
            cli.verbose,
            cli.max_errors,
        ),
        Some(Commands::Eval { expr }) => cmd_eval(&expr, cli.verbose, cli.max_errors),
        Some(Commands::Repl) => cmd_repl(),
        Some(Commands::Clean) => cmd_clean(cli.verbose),
        Some(Commands::Cache(CacheCommands::Stats { json })) => cmd_cache_stats(cli.verbose, json),
        Some(Commands::Cache(CacheCommands::Prune { target_mb })) => {
            cmd_cache_prune(cli.verbose, target_mb)
        }
        None => {
            // No file and no command — show help
            use clap::CommandFactory;
            Cli::command().print_help().unwrap();
            println!();
            ExitCode::SUCCESS
        }
    }
}

/// Check command: type-check a file.
fn cmd_check(file: &PathBuf, verbose: bool, no_cache: bool, max_errors: usize) -> ExitCode {
    match driver::run_file_with_options(file, Mode::Check, verbose, no_cache, max_errors) {
        Ok(PipelineResult::Checked {
            num_defs,
            has_sorry,
        }) => {
            if has_sorry {
                println!(
                    "⚠ {}: {} definition(s), contains sorry",
                    file.display(),
                    num_defs
                );
            } else {
                println!("✓ {}: {} definition(s), all OK", file.display(), num_defs);
            }
            ExitCode::SUCCESS
        }
        Ok(PipelineResult::Failed) => ExitCode::FAILURE,
        Ok(PipelineResult::Evaluated { .. }) => {
            // Shouldn't happen in check mode
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(3) // IO error
        }
    }
}

/// Compile command: compile to native executable.
#[cfg(feature = "codegen")]
fn cmd_compile(
    file: &PathBuf,
    output: Option<&std::path::Path>,
    emit_llvm: bool,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    use std::fs;
    use std::process::Command;
    use tungsten_codegen::inkwell::context::Context as LlvmContext;
    use tungsten_codegen::CodeGen;

    // Use driver's elaborate_project for multi-module support
    let (defs, record_types, adt_types, _source_map) =
        match driver::elaborate_project(file, verbose, max_errors) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("error: {}", e);
                return ExitCode::FAILURE;
            }
        };

    if verbose {
        eprintln!("Elaborated {} definition(s)", defs.len());
        if !record_types.is_empty() {
            eprintln!("Found {} record type(s)", record_types.len());
        }
        if !adt_types.is_empty() {
            eprintln!("Found {} ADT type(s)", adt_types.len());
        }
    }

    // Convert bootstrap Constructor to codegen CodegenConstructor
    let codegen_adt_types: std::collections::HashMap<
        String,
        (Vec<String>, Vec<tungsten_codegen::CodegenConstructor>),
    > = adt_types
        .into_iter()
        .map(|(name, (params, constructors))| {
            let codegen_ctors: Vec<tungsten_codegen::CodegenConstructor> = constructors
                .into_iter()
                .map(|ctor| tungsten_codegen::CodegenConstructor {
                    name: ctor.name,
                    fields: ctor.fields,
                    index: ctor.index,
                })
                .collect();
            (name, (params, codegen_ctors))
        })
        .collect();

    // Check for sorry
    // TODO: Track sorry spans during elaboration for better error location
    // TODO: Pattern matching generates Term::Sorry in absurd branches (dead code).
    //       Should use Term::Absurd or similar instead. For now, warn but continue.
    let source = fs::read_to_string(file).unwrap_or_default();
    let eof_span = tungsten_bootstrap::span::Span::new(source.len() as u32, source.len() as u32);
    let sorry_defs: Vec<_> = defs.iter().filter(|d| contains_sorry(&d.term)).collect();
    if !sorry_defs.is_empty() {
        eprintln!(
            "warning: {} definition(s) contain `sorry` (may be dead code from pattern matching):",
            sorry_defs.len()
        );
        for def in &sorry_defs {
            eprintln!("  - {}", def.name);
        }
        // Continue anyway - sorry in dead code is acceptable for compilation
    }

    // Find main function
    let main_def = match defs.iter().find(|d| d.name == "main") {
        Some(d) => d,
        None => {
            let err = tungsten_bootstrap::ElabError::no_main_function(eof_span);
            driver::render_diagnostics(&source, &file.to_string_lossy(), &[err], &[]);
            return ExitCode::FAILURE;
        }
    };

    // Generate LLVM IR
    let llvm_context = LlvmContext::create();
    let module_name = file.file_stem().unwrap_or_default().to_string_lossy();
    let mut codegen = CodeGen::new(&llvm_context, &module_name);

    // Register type definitions so codegen can expand nominal types to structural types
    codegen.register_record_types(record_types);
    codegen.register_adt_types(codegen_adt_types);

    // Track extern wrapper name mappings: original_name -> llvm_name
    // Extern wrappers are renamed to avoid shadowing C runtime symbols.
    let mut extern_name_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // Pass 1: Declare all functions (add prototypes to enable forward references)
    // Extern wrappers are renamed to __wrap_<name> to avoid shadowing C runtime.
    for def in &defs {
        let original_name = if def.name == "main" {
            "tungsten_main".to_string()
        } else {
            def.name.clone()
        };

        // Check if this is an extern wrapper
        let llvm_name = if let Some(extern_symbol) = get_extern_symbol(&def.term) {
            // This is an extern wrapper - use a prefixed name
            // The extern symbol is __c_<raw_name>, so the raw name is after __c_
            let raw_name = extern_symbol.strip_prefix("__c_").unwrap_or(&extern_symbol);
            let prefixed = format!("__wrap_{}", raw_name);
            extern_name_map.insert(original_name.clone(), prefixed.clone());
            prefixed
        } else {
            original_name.clone()
        };

        if let Err(e) = codegen.declare_def(&llvm_name, &def.ty) {
            eprintln!("error: declaration failed for '{}': {}", def.name, e);
            return ExitCode::FAILURE;
        }
    }

    // Register extern name mappings with codegen for Global lookups
    codegen.register_extern_name_map(extern_name_map.clone());

    // Register term definitions for potential monomorphization
    // This allows the codegen to re-compile polymorphic functions with concrete types
    for def in &defs {
        let original_name = if def.name == "main" {
            "tungsten_main".to_string()
        } else {
            def.name.clone()
        };
        let llvm_name = extern_name_map
            .get(&original_name)
            .cloned()
            .unwrap_or(original_name);
        codegen.register_term_def(&llvm_name, def.term.clone());
    }

    // Pass 2: Compile all definitions
    let total_defs = defs.len();
    for (i, def) in defs.iter().enumerate() {
        let original_name = if def.name == "main" {
            "tungsten_main".to_string()
        } else {
            def.name.clone()
        };

        // Use renamed name for extern wrappers
        let llvm_name = extern_name_map
            .get(&original_name)
            .cloned()
            .unwrap_or(original_name);

        if verbose {
            eprintln!("Compiling {} [{}/{}]...", def.name, i + 1, total_defs);
        }

        if let Err(e) = codegen.compile_def(&llvm_name, &def.term, &def.ty) {
            eprintln!("error: codegen failed for '{}': {}", def.name, e);
            return ExitCode::FAILURE;
        }
    }

    // Create main wrapper
    if verbose {
        eprintln!("Creating main wrapper...");
    }
    if let Err(e) = codegen.compile_main_wrapper(&main_def.ty) {
        eprintln!("error: could not create main wrapper: {}", e);
        return ExitCode::FAILURE;
    }

    // Emit LLVM IR if requested
    if emit_llvm {
        let ir = codegen.get_ir_string();
        let output_path = output
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| file.with_extension("ll"));

        if let Err(e) = fs::write(&output_path, ir) {
            eprintln!("error: could not write '{}': {}", output_path.display(), e);
            return ExitCode::from(3);
        }

        println!("✓ Wrote LLVM IR to {}", output_path.display());
        return ExitCode::SUCCESS;
    }

    // Write object file
    if verbose {
        eprintln!("Writing object file...");
    }
    let obj_path = file.with_extension("o");
    if let Err(e) = codegen.write_object_file(&obj_path) {
        eprintln!("error: could not write object file: {}", e);
        return ExitCode::FAILURE;
    }

    if verbose {
        eprintln!("Wrote object file to {}", obj_path.display());
    }

    // Link with system linker
    if verbose {
        eprintln!("Linking...");
    }
    let exe_path = output.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        #[cfg(target_os = "windows")]
        {
            file.with_extension("exe")
        }
        #[cfg(not(target_os = "windows"))]
        {
            file.with_extension("")
        }
    });

    // Find the tungsten_core library directory
    // First, try relative to the compiler executable
    let lib_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("target/release"));

    let mut link_cmd = Command::new("cc");
    link_cmd.arg(&obj_path).arg("-o").arg(&exe_path);

    // Add library search path and link tungsten_core
    link_cmd.arg(format!("-L{}", lib_dir.display()));
    link_cmd.arg("-ltungsten_core");

    // Set rpath for runtime library discovery
    #[cfg(target_os = "macos")]
    {
        link_cmd.arg(format!("-Wl,-rpath,{}", lib_dir.display()));
    }
    #[cfg(target_os = "linux")]
    {
        link_cmd.arg(format!("-Wl,-rpath,{}", lib_dir.display()));
        link_cmd.arg("-Wl,--enable-new-dtags");
    }

    if verbose {
        eprintln!("Library directory: {}", lib_dir.display());
    }

    let status = link_cmd.status();

    // Clean up object file
    let _ = fs::remove_file(&obj_path);

    match status {
        Ok(s) if s.success() => {
            println!("✓ Compiled to {}", exe_path.display());
            ExitCode::SUCCESS
        }
        Ok(s) => {
            eprintln!("error: linker failed with status {}", s);
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: could not run linker: {}", e);
            eprintln!("help: make sure 'cc' (C compiler) is installed and in PATH");
            ExitCode::FAILURE
        }
    }
}

/// Check if a term is a pure extern wrapper (lambdas wrapping ExternCall).
/// Returns the extern symbol if it is, None otherwise.
#[cfg(feature = "codegen")]
fn get_extern_symbol(term: &tungsten_core::Term) -> Option<String> {
    use tungsten_core::Term;
    match term {
        Term::ExternCall(symbol, _) => Some(symbol.clone()),
        Term::Lambda(_, _, body) => get_extern_symbol(body),
        _ => None,
    }
}

/// Check if a term contains sorry.
#[cfg(feature = "codegen")]
fn contains_sorry(term: &tungsten_core::Term) -> bool {
    use tungsten_core::Term;
    match term {
        Term::Sorry => true,
        Term::Var(_) | Term::Unit | Term::True | Term::False | Term::Zero => false,
        Term::Succ(n) => contains_sorry(n),
        Term::Lambda(_, _, body) | Term::TyAbs(_, body) => contains_sorry(body),
        Term::App(f, x) | Term::Pair(f, x) => contains_sorry(f) || contains_sorry(x),
        Term::TyApp(t, _) | Term::Fst(t) | Term::Snd(t) => contains_sorry(t),
        Term::Inl(_, t) | Term::Inr(_, t) => contains_sorry(t),
        Term::Let(_, _, v, b) => contains_sorry(v) || contains_sorry(b),
        Term::If(c, t, e) => contains_sorry(c) || contains_sorry(t) || contains_sorry(e),
        Term::Case(s, _, l, _, r) => contains_sorry(s) || contains_sorry(l) || contains_sorry(r),
        Term::NatRec(_, z, s, n) => contains_sorry(z) || contains_sorry(s) || contains_sorry(n),
        Term::NatInd(_, z, s, n) => contains_sorry(z) || contains_sorry(s) || contains_sorry(n),
        Term::Refl(_, t) => contains_sorry(t),
        Term::Subst(_, _, eq, body) => contains_sorry(eq) || contains_sorry(body),
        Term::Absurd(_, t) => contains_sorry(t),
        Term::Annot(t, _) => contains_sorry(t),
        Term::StringLit(_) => false,
        Term::StrConcat(a, b) | Term::StrEq(a, b) | Term::StrCharAt(a, b) => {
            contains_sorry(a) || contains_sorry(b)
        }
        Term::StrLen(t) => contains_sorry(t),
        Term::StrSubstring(s, start, len) => {
            contains_sorry(s) || contains_sorry(start) || contains_sorry(len)
        }
        Term::Fix(_, _, body) => contains_sorry(body),
        Term::Fold(_, t) | Term::Unfold(_, t) => contains_sorry(t),
        // Phase 3-Prep: Globals, Nat literals, comparisons, refs, externs
        Term::Global(_) | Term::NatLit(_) => false,
        Term::NatLt(a, b) | Term::NatLe(a, b) | Term::NatGt(a, b) | Term::NatGe(a, b) => {
            contains_sorry(a) || contains_sorry(b)
        }
        // Phase 3C: Nat arithmetic and equality
        Term::NatAdd(a, b)
        | Term::NatSub(a, b)
        | Term::NatMul(a, b)
        | Term::NatDiv(a, b)
        | Term::NatMod(a, b)
        | Term::NatEq(a, b) => contains_sorry(a) || contains_sorry(b),
        // Phase 3C: Boolean operators
        Term::BoolAnd(a, b) | Term::BoolOr(a, b) => contains_sorry(a) || contains_sorry(b),
        Term::BoolNot(t) => contains_sorry(t),
        Term::ExternCall(_, args) => args.iter().any(contains_sorry),
        Term::RefNew(t) | Term::RefGet(t) => contains_sorry(t),
        Term::RefSet(r, v) => contains_sorry(r) || contains_sorry(v),
        // ADR 2.2.26: Flat ADT operations
        Term::AdtConstruct(_, _, payload) => contains_sorry(payload),
        Term::AdtMatch(scrutinee, arms) => {
            contains_sorry(scrutinee) || arms.iter().any(|(_, _, body)| contains_sorry(body))
        }
    }
}

/// Run command: type-check and evaluate `main()`.
fn cmd_run(file: &PathBuf, verbose: bool, no_cache: bool, max_errors: usize) -> ExitCode {
    match driver::run_file_with_options(file, Mode::Run, verbose, no_cache, max_errors) {
        Ok(PipelineResult::Evaluated { value, ty }) => {
            let value_str = driver::format_value(&value);
            if verbose {
                let ty_str = driver::format_type(&ty);
                println!("{value_str} : {ty_str}");
            } else {
                println!("{value_str}");
            }
            ExitCode::SUCCESS
        }
        Ok(PipelineResult::Checked { .. }) => {
            // Shouldn't happen in run mode
            ExitCode::SUCCESS
        }
        Ok(PipelineResult::Failed) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(3)
        }
    }
}

/// Eval command: evaluate a single expression.
fn cmd_eval(expr: &str, verbose: bool, max_errors: usize) -> ExitCode {
    match driver::eval_expr(expr, verbose, max_errors) {
        Ok(PipelineResult::Evaluated { value, ty }) => {
            let value_str = driver::format_value(&value);
            if verbose {
                let ty_str = driver::format_type(&ty);
                println!("{value_str} : {ty_str}");
            } else {
                println!("{value_str}");
            }
            ExitCode::SUCCESS
        }
        Ok(PipelineResult::Failed) => ExitCode::FAILURE,
        Ok(PipelineResult::Checked { .. }) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(3)
        }
    }
}

/// REPL command: interactive mode.
fn cmd_repl() -> ExitCode {
    eprintln!("Tungsten {} — Interactive Mode", env!("CARGO_PKG_VERSION"));
    eprintln!("Type expressions to evaluate, or :help for commands.");
    eprintln!();

    // Simple REPL without rustyline for now
    use std::io::{self, BufRead, Write};

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("tg> ");
        stdout.flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                // EOF
                println!();
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("error: {e}");
                break;
            }
        }

        let line = line.trim();

        // Handle REPL commands
        if line.starts_with(':') {
            match line {
                ":quit" | ":q" | ":exit" => break,
                ":help" | ":h" | ":?" => {
                    println!("Commands:");
                    println!("  :help, :h, :?    Show this help");
                    println!("  :quit, :q        Exit the REPL");
                    println!();
                    println!("Enter expressions to evaluate them.");
                }
                _ => {
                    eprintln!("Unknown command: {line}");
                    eprintln!("Type :help for available commands.");
                }
            }
            continue;
        }

        if line.is_empty() {
            continue;
        }

        // Evaluate the expression (use default max_errors in REPL)
        match driver::eval_expr(line, false, 20) {
            Ok(PipelineResult::Evaluated { value, ty }) => {
                let value_str = driver::format_value(&value);
                let ty_str = driver::format_type(&ty);
                println!("{value_str} : {ty_str}");
            }
            Ok(PipelineResult::Failed) => {
                // Error already printed
            }
            Ok(PipelineResult::Checked { .. }) => {}
            Err(e) => {
                eprintln!("error: {e}");
            }
        }
    }

    ExitCode::SUCCESS
}

/// Clean command: clear the build cache.
fn cmd_clean(verbose: bool) -> ExitCode {
    use std::env;
    use tungsten_bootstrap::cache::BuildCache;

    let cwd = match env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not get current directory: {e}");
            return ExitCode::from(3);
        }
    };

    let mut cache = match BuildCache::new(&cwd, verbose) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not open cache: {e}");
            return ExitCode::from(3);
        }
    };

    match cache.clear() {
        Ok(()) => {
            println!("✓ Cache cleared");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: could not clear cache: {e}");
            ExitCode::from(3)
        }
    }
}

/// Cache stats command: show cache statistics.
fn cmd_cache_stats(verbose: bool, json: bool) -> ExitCode {
    use std::env;
    use tungsten_bootstrap::cache::BuildCache;

    let cwd = match env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not get current directory: {e}");
            return ExitCode::from(3);
        }
    };

    let cache = match BuildCache::new(&cwd, verbose) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not open cache: {e}");
            return ExitCode::from(3);
        }
    };

    let stats = match cache.stats() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not get cache stats: {e}");
            return ExitCode::from(3);
        }
    };

    if json {
        // JSON output for scripting
        let oldest_ms = stats.oldest_accessed.map_or(0, |d| d.as_millis());
        let newest_ms = stats.newest_accessed.map_or(0, |d| d.as_millis());
        println!(
            r#"{{"size_bytes":{},"entry_count":{},"max_size_mb":{},"oldest_accessed_ms":{},"newest_accessed_ms":{}}}"#,
            stats.size_bytes, stats.entry_count, stats.max_size_mb, oldest_ms, newest_ms
        );
    } else {
        // Human-readable output
        let size_kb = stats.size_bytes / 1024;
        let size_mb = stats.size_bytes / (1024 * 1024);

        println!("Cache Statistics:");
        println!("  Entries:    {}", stats.entry_count);
        if size_mb > 0 {
            println!("  Size:       {size_mb} MB ({size_kb} KB)");
        } else {
            println!("  Size:       {size_kb} KB");
        }
        println!("  Max size:   {} MB", stats.max_size_mb);

        if let Some(oldest) = stats.oldest_accessed {
            println!("  Oldest:     {} ago", format_duration_ago(oldest));
        }
        if let Some(newest) = stats.newest_accessed {
            println!("  Newest:     {} ago", format_duration_ago(newest));
        }
    }

    ExitCode::SUCCESS
}

/// Format a duration as a human-readable "X ago" string.
fn format_duration_ago(timestamp: std::time::Duration) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let elapsed = now.saturating_sub(timestamp);
    let secs = elapsed.as_secs();

    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

/// Cache prune command: remove least recently used entries.
fn cmd_cache_prune(verbose: bool, target_mb: Option<u64>) -> ExitCode {
    use std::env;
    use tungsten_bootstrap::cache::BuildCache;

    let cwd = match env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not get current directory: {e}");
            return ExitCode::from(3);
        }
    };

    let mut cache = match BuildCache::new(&cwd, verbose) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not open cache: {e}");
            return ExitCode::from(3);
        }
    };

    match cache.prune(target_mb) {
        Ok(stats) => {
            if stats.removed_count == 0 {
                println!(
                    "✓ Cache already within limits ({} KB)",
                    stats.new_size_bytes / 1024
                );
            } else {
                println!(
                    "✓ Pruned {} entries, freed {} KB (new size: {} KB)",
                    stats.removed_count,
                    stats.freed_bytes / 1024,
                    stats.new_size_bytes / 1024
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: could not prune cache: {e}");
            ExitCode::from(3)
        }
    }
}
