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
        DoctorCommands::AuditRecursion {
            file,
            source_only: _,
        } => {
            // No-codegen build: always the source-level estimate. The
            // codegen-consulting path is dispatched binary-side (ADR 1.7.26b §2.2).
            audit_recursion::cmd_audit_recursion(&file, global_verbose, 20)
        }
        DoctorCommands::AuditMutualTypes { file, json } => {
            audit_mutual_types::cmd_audit_mutual_types(&file, global_verbose, 20, json)
        }
        DoctorCommands::AuditDeadDefinitions { file, roots } => {
            audit_dead_definitions::cmd_audit_dead_definitions(&file, global_verbose, 20, &roots)
        }
        DoctorCommands::AuditDriverReach { file, test_entries } => {
            audit_driver_reach::cmd_audit_driver_reach(&file, &test_entries)
        }
        DoctorCommands::AuditOrphanSources { file } => {
            audit_orphan_sources::cmd_audit_orphan_sources(&file)
        }
        DoctorCommands::DiffTypes {
            type_a,
            type_b,
            file,
        } => diff_types::cmd_diff_types(&type_a, &type_b, &file, global_verbose, 20),
        DoctorCommands::ToolReachability => {
            checks::check_tool_reachability::cmd_check_tool_reachability(global_verbose)
        }
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
        CheckCommands::Module(sub) => check_module::dispatch_check_module(sub, verbose),
        CheckCommands::Link(sub) => check_link::dispatch_check_link(sub, verbose),
        CheckCommands::Selfhost(sub) => check_selfhost::dispatch_check_selfhost(sub, verbose),
        // Handled binary-side, where the codegen handlers live. The grouped
        // spelling is rewritten onto these by `check_codegen::flatten` before
        // dispatch, so reaching either arm means the binary did not claim it.
        #[cfg(feature = "codegen")]
        CheckCommands::Codegen(_)
        | CheckCommands::MonoCoverage { .. }
        | CheckCommands::ExternMapAmbiguity { .. }
        | CheckCommands::TcoCoverage { .. }
        | CheckCommands::UnitCost { .. } => {
            eprintln!("ICE: `doctor check codegen` should be dispatched from the binary crate");
            ExitCode::FAILURE
        }
        CheckCommands::SelfCompileReadiness => {
            checks::check_self_compile_readiness::cmd_check_self_compile_readiness(verbose)
        }
        CheckCommands::NestedPatterns { file } => {
            checks::check_nested_patterns::cmd_check_nested_patterns(&file, verbose)
        }
        CheckCommands::SorrySites { file, json } => {
            checks::check_sorry_sites::cmd_check_sorry_sites(&file, json, verbose)
        }
        CheckCommands::ExternCoverage { file } => {
            checks::check_extern_coverage::cmd_check_extern_coverage(&file, verbose)
        }
        CheckCommands::Comparable {
            type_name,
            file,
            all,
        } => checks::check_comparable::run(type_name, file, all, verbose),
        // Legacy aliases delegate to same handlers (ADR 12.5.26h §2.3).
        legacy => dispatch_legacy_check(legacy, verbose),
    }
}

/// Dispatch hidden legacy check alias commands.
fn dispatch_legacy_check(cmd: CheckCommands, verbose: bool) -> ExitCode {
    match cmd {
        CheckCommands::NormalizationConsistencyLegacy { file } => {
            checks::check_normalization::cmd_check_normalization_consistency(
                &file, verbose, 20, false,
            )
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
        #[cfg(feature = "codegen")]
        CheckCommands::LinkCollisionsLegacy { dir } => {
            checks::check_link_collisions::cmd_check_link_collisions(&dir)
        }
        CheckCommands::LinkHealthLegacy { binary } => {
            checks::check_link_health::cmd_check_link_health(&binary, verbose)
        }
        // ADR 29.8.26a D2: the flat spellings of the four `check module`
        // members. Each delegates to the same handler its grouped twin does —
        // not to the grouped variant — so an alias cannot drift into meaning
        // something slightly different from the command it aliases.
        CheckCommands::ReexportCompletenessLegacy { file } => {
            checks::check_reexport_completeness::cmd_check_reexport_completeness(&file, verbose)
        }
        CheckCommands::NameCollisionsLegacy {
            file,
            severity,
            json,
            include_reexports,
        } => checks::check_name_collisions::cmd_check_name_collisions(
            &file,
            severity,
            json,
            include_reexports,
        ),
        CheckCommands::ModuleOverlapLegacy { path, json } => {
            module_overlap::cmd_check_module_overlap(path.as_deref(), json)
        }
        CheckCommands::SignatureCollectionLegacy { file } => {
            checks::check_signature_collection::cmd_check_signature_collection(&file, verbose)
        }
        CheckCommands::DeclaresLegacy { from_existing_ir } => {
            // The legacy alias predates `--strict` (ADR 28.7.26e) and keeps the
            // permissive default; `doctor check ir declares --strict` is the
            // gated form.
            checks::check_declares::cmd_check_declares(&from_existing_ir, false)
        }
        _ => unreachable!("all non-legacy check commands matched in dispatch_check_command"),
    }
}
