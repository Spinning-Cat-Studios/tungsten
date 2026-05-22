//! Command dispatch for `doctor` and `doctor check` subcommands.

use std::process::ExitCode;

use super::*;

/// Dispatch a doctor subcommand.
pub fn cmd_doctor(cmd: DoctorCommands, global_verbose: bool) -> ExitCode {
    match cmd {
        DoctorCommands::SelfTest {
            full,
            verbose,
            json,
        } => self_test::cmd_self_test(full, verbose || global_verbose, json),
        DoctorCommands::AuditRecursion { file } => {
            audit_recursion::cmd_audit_recursion(&file, global_verbose, 20)
        }
        DoctorCommands::AuditMutualTypes { file, json } => {
            audit_mutual_types::cmd_audit_mutual_types(&file, global_verbose, 20, json)
        }
        DoctorCommands::DiffTypes {
            type_a,
            type_b,
            file,
        } => diff_types::cmd_diff_types(&type_a, &type_b, &file, global_verbose, 20),
        DoctorCommands::SuggestTools { description, json } => {
            suggest_tools::cmd_suggest_tools(&description, json)
        }
        DoctorCommands::MapSpan {
            file,
            offset,
            project,
        } => map_span::cmd_map_span(&file, offset, project, global_verbose),
        DoctorCommands::Check(subcmd) => dispatch_check_command(subcmd, global_verbose),
    }
}

/// Dispatch `doctor check` subcommands.
fn dispatch_check_command(cmd: CheckCommands, verbose: bool) -> ExitCode {
    match cmd {
        CheckCommands::Type(sub) => check_type::dispatch_check_type(sub, verbose),
        CheckCommands::Ir(sub) => check_ir::dispatch_check_ir(sub),
        CheckCommands::ReexportCompleteness { file } => {
            checks::check_reexport_completeness::cmd_check_reexport_completeness(&file, verbose)
        }
        #[cfg(feature = "codegen")]
        CheckCommands::LinkCollisions { dir } => {
            checks::check_link_collisions::cmd_check_link_collisions(&dir)
        }
        #[cfg(feature = "codegen")]
        CheckCommands::MonoCoverage { file } => {
            eprintln!("ICE: mono-coverage should be dispatched from the binary crate");
            let _ = file;
            ExitCode::FAILURE
        }
        CheckCommands::ModuleOverlap { path, json } => {
            module_overlap::cmd_check_module_overlap(path.as_deref(), json)
        }
        CheckCommands::PhaseA5 { file } => {
            checks::check_phase_a5::cmd_check_phase_a5(&file, verbose)
        }
        CheckCommands::LinkHealth { binary } => {
            checks::check_link_health::cmd_check_link_health(&binary, verbose)
        }
        CheckCommands::SelfCompileReadiness => {
            checks::check_self_compile_readiness::cmd_check_self_compile_readiness(verbose)
        }
        CheckCommands::NestedPatterns { file } => {
            checks::check_nested_patterns::cmd_check_nested_patterns(&file, verbose)
        }
        // Legacy aliases delegate to same handlers (ADR 12.5.26h §2.3).
        legacy => dispatch_legacy_check(legacy, verbose),
    }
}

/// Dispatch hidden legacy check alias commands.
fn dispatch_legacy_check(cmd: CheckCommands, verbose: bool) -> ExitCode {
    match cmd {
        CheckCommands::NormalizationConsistencyLegacy { file } => {
            checks::check_normalization::cmd_check_normalization_consistency(&file, verbose, 20)
        }
        CheckCommands::EncodingDepthLegacy(args) => {
            let thresholds = checks::check_encoding_depth::DepthThresholds {
                max_stack: args.max_stack,
                max_depth: args.max_depth,
                max_nodes: args.max_nodes,
            };
            checks::check_encoding_depth::cmd_check_encoding_depth(
                &args.file,
                verbose,
                20,
                &thresholds,
            )
        }
        CheckCommands::TypeSizesLegacy { file, max_nodes } => {
            checks::check_type_sizes::cmd_check_type_sizes(&file, verbose, 20, max_nodes)
        }
        CheckCommands::PhaseInvariantsLegacy { file } => {
            checks::check_phase_invariants::cmd_check_phase_invariants(&file, verbose, 20)
        }
        CheckCommands::FoldConsistencyLegacy { file, json } => {
            checks::check_fold_consistency::cmd_check_fold_consistency(&file, verbose, 20, json)
        }
        CheckCommands::IrLayoutLegacy { file, json } => {
            checks::check_ir_layout::cmd_check_ir_layout(&file, json)
        }
        CheckCommands::StubsLegacy { file } => {
            checks::check_stubs::cmd_check_stubs(&file, verbose, 20)
        }
        CheckCommands::ConstructorCountsLegacy { file, json } => {
            checks::check_constructor_counts::cmd_check_constructor_counts(&file, verbose, 20, json)
        }
        CheckCommands::DeclaresLegacy { from_existing_ir } => {
            checks::check_declares::cmd_check_declares(&from_existing_ir)
        }
        _ => unreachable!("all non-legacy check commands matched in dispatch_check_command"),
    }
}
