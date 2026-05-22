use super::*;

#[test]
#[cfg(not(target_os = "macos"))]
fn test_lld_available_is_consistent() {
    // OnceLock caching: repeated calls must return the same value.
    let first = lld_available();
    let second = lld_available();
    assert_eq!(
        first, second,
        "lld_available should be deterministic (OnceLock)"
    );
}

#[test]
fn test_linker_command_exe_path_default() {
    let file = Path::new("/tmp/test.tg");
    let cmd = LinkerCommand::new(file, None, false, false);
    // Default: strip .tg extension
    assert_eq!(cmd.exe_path, PathBuf::from("/tmp/test"));
}

#[test]
fn test_linker_command_exe_path_custom_output() {
    let file = Path::new("/tmp/test.tg");
    let output = Path::new("/out/binary");
    let cmd = LinkerCommand::new(file, Some(output), false, false);
    assert_eq!(cmd.exe_path, PathBuf::from("/out/binary"));
}

#[test]
fn test_build_cc_preserves_input_order() {
    let file = Path::new("/tmp/test.tg");
    let linker = LinkerCommand::new(file, None, false, false);
    let inputs = vec![
        PathBuf::from("/tmp/b.o"),
        PathBuf::from("/tmp/a.o"),
        PathBuf::from("/tmp/c.o"),
    ];
    let cmd = linker.build_cc(&inputs);
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    // Inputs appear in the order provided (not sorted)
    let b_pos = args.iter().position(|a| a == "/tmp/b.o").unwrap();
    let a_pos = args.iter().position(|a| a == "/tmp/a.o").unwrap();
    let c_pos = args.iter().position(|a| a == "/tmp/c.o").unwrap();
    assert!(b_pos < a_pos, "b.o should appear before a.o");
    assert!(a_pos < c_pos, "a.o should appear before c.o");
}

#[test]
fn test_build_cc_includes_sanitize_flag() {
    let file = Path::new("/tmp/test.tg");
    let linker = LinkerCommand::new(file, None, true, false);
    let inputs = vec![PathBuf::from("/tmp/test.o")];
    let cmd = linker.build_cc(&inputs);
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    assert!(
        args.contains(&"-fsanitize=address".to_string()),
        "sanitized LinkerCommand should include -fsanitize=address"
    );
}

#[test]
fn test_build_cc_no_sanitize_by_default() {
    let file = Path::new("/tmp/test.tg");
    let linker = LinkerCommand::new(file, None, false, false);
    let inputs = vec![PathBuf::from("/tmp/test.o")];
    let cmd = linker.build_cc(&inputs);
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    assert!(
        !args.contains(&"-fsanitize=address".to_string()),
        "non-sanitized LinkerCommand should not include -fsanitize=address"
    );
}

#[test]
fn test_build_cc_includes_static_library() {
    let file = Path::new("/tmp/test.tg");
    let linker = LinkerCommand::new(file, None, false, false);
    let inputs = vec![PathBuf::from("/tmp/test.o")];
    let cmd = linker.build_cc(&inputs);
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    // Static linking (ADR 18.5.26e): full path to .a archive
    assert!(
        args.iter().any(|a| a.ends_with("libtungsten_core.a")),
        "LinkerCommand should include libtungsten_core.a static archive, got: {:?}",
        args,
    );
}

#[test]
fn test_build_cc_includes_platform_libs() {
    let file = Path::new("/tmp/test.tg");
    let linker = LinkerCommand::new(file, None, false, false);
    let inputs = vec![PathBuf::from("/tmp/test.o")];
    let cmd = linker.build_cc(&inputs);
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    // Platform-specific transitive C deps should be present
    #[cfg(target_os = "macos")]
    assert!(
        args.contains(&"-lSystem".to_string()),
        "should include platform libs on macOS, got: {:?}",
        args,
    );
    #[cfg(target_os = "linux")]
    assert!(
        args.contains(&"-lpthread".to_string()),
        "should include platform libs on Linux, got: {:?}",
        args,
    );
}

#[test]
fn test_build_cc_includes_stack_size() {
    let file = Path::new("/tmp/test.tg");
    let linker = LinkerCommand::new(file, None, false, false);
    let inputs = vec![PathBuf::from("/tmp/test.o")];
    let cmd = linker.build_cc(&inputs);
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    // 128 MB stack size must be set to prevent stack overflow during
    // elaboration of large programs (e.g., the compiler checking itself).
    // See ADR 20.5.26e (tail-recursive list operations).
    #[cfg(target_os = "macos")]
    assert!(
        args.contains(&"-Wl,-stack_size,0x8000000".to_string()),
        "should include stack_size linker flag on macOS, got: {:?}",
        args,
    );
    #[cfg(target_os = "linux")]
    assert!(
        args.contains(&"-Wl,-z,stack-size=134217728".to_string()),
        "should include stack-size linker flag on Linux, got: {:?}",
        args,
    );
}
