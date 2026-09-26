//! Tests for the `main` / `__tungsten_inner_main` wrapper: what the prologue
//! emits unconditionally.

use super::*;
use inkwell::context::Context;

/// Verify that `compile_main_wrapper` unconditionally emits the CLI args
/// init sequence — `tg_init_args_c` call + `__tungsten_argc`/`__tungsten_argv`
/// globals — even when `tg_argc`/`tg_argv` are not present in the module.
///
/// ADR 10.5.26c: in per-module codegen, `tg_argc`/`tg_argv` live in separate
/// codegen units, so a visibility check would incorrectly skip init.
#[test]
fn test_emit_cli_args_init_unconditional() {
    let ctx = Context::create();
    let mut codegen = CodeGen::new(&ctx, "test_main_wrapper");

    // Register a simple main type: Unit -> Unit
    let main_ty = tungsten_core::types::Type::Arrow(
        Box::new(tungsten_core::types::Type::Unit),
        Box::new(tungsten_core::types::Type::Unit),
    );

    // Declare tungsten_main so the wrapper can call it
    codegen
        .declare_def("tungsten_main", &main_ty)
        .expect("declare tungsten_main");

    // Compile the main wrapper — should succeed without tg_argc/tg_argv
    codegen
        .compile_main_wrapper(&main_ty)
        .expect("compile_main_wrapper should succeed");

    let ir = codegen.get_ir_string();

    // Verify tg_init_args_c is declared and called
    assert!(
        ir.contains("tg_init_args_c"),
        "IR should contain tg_init_args_c declaration/call"
    );
    // Verify argc/argv globals are emitted
    assert!(
        ir.contains("__tungsten_argc"),
        "IR should contain __tungsten_argc global"
    );
    assert!(
        ir.contains("__tungsten_argv"),
        "IR should contain __tungsten_argv global"
    );
}

/// ADR 14.9.26b: the prologue reads `TUNGSTEN_ARENA` through
/// `__tungsten_arena_init()` unconditionally — including under
/// `--debug-info`, where the signal-handler install is skipped and a
/// mode read placed beside it would leave a debug build silently `off`.
/// The call must precede the `tungsten_main` call, so the first
/// allocation sees the mode.
#[test]
fn arena_init_is_emitted_before_main_even_with_debug_info() {
    let ctx = Context::create();
    let mut codegen = CodeGen::new(&ctx, "test_arena_init");
    codegen.enable_debug_info("arena_init.tg", "fn main() -> Unit { () }\n");

    let main_ty = tungsten_core::types::Type::Arrow(
        Box::new(tungsten_core::types::Type::Unit),
        Box::new(tungsten_core::types::Type::Unit),
    );
    codegen
        .declare_def("tungsten_main", &main_ty)
        .expect("declare tungsten_main");
    codegen
        .compile_main_wrapper(&main_ty)
        .expect("compile_main_wrapper should succeed");

    let ir = codegen.get_ir_string();
    assert!(
        !ir.contains("__tungsten_install_signal_handlers"),
        "precondition: debug-info skips the signal handlers"
    );
    let init_at = ir
        .find("call void @__tungsten_arena_init()")
        .expect("IR should call __tungsten_arena_init()");
    // The call site passes the null env; the declaration has no operand.
    let main_at = ir
        .find("@tungsten_main(ptr null)")
        .expect("IR should call tungsten_main");
    assert!(
        init_at < main_at,
        "arena init must run before the first allocation (tungsten_main)"
    );
}
