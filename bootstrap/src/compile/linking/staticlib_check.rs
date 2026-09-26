//! Pre-link platform check for `libtungsten_core.a` (ADR 7.7.26k friction
//! follow-up).
//!
//! The `target/` directory is bind-mount shared host↔devcontainer
//! (ADR 2.7.26b), so a container build can leave a GNU-format (Linux)
//! archive that a macOS host link then rejects with the cryptic
//! `ld: archive member '/' not a mach-o file` — and vice versa. Detecting
//! the foreign archive format up front turns a wasted debugging round-trip
//! into a one-line diagnosis with the fix command.
//!
//! Detection is deliberately conservative and FAIL-OPEN: only the two
//! unambiguous first-member signatures are treated as mismatches; anything
//! unreadable or unrecognized falls through to the real linker.

use std::path::Path;

/// Magic prefix every ar archive starts with.
const AR_MAGIC: &[u8] = b"!<arch>\n";

/// Byte offset of the first member header (right after the magic).
const FIRST_MEMBER_NAME_RANGE: std::ops::Range<usize> = 8..24;

/// If the archive at `lib_path` was built for the OTHER platform, return a
/// ready-to-print diagnosis (with the ADR 2.7.26b clobber explanation and
/// the rebuild command). Returns `None` when the file looks native,
/// unreadable, or unrecognized — the real linker stays the authority.
pub(super) fn foreign_platform_diagnosis(lib_path: &Path) -> Option<String> {
    let mut prefix = [0u8; 24];
    let bytes_read = {
        use std::io::Read;
        let mut file = std::fs::File::open(lib_path).ok()?;
        file.read(&mut prefix).ok()?
    };
    let foreign_format = foreign_format_name(&prefix[..bytes_read], cfg!(target_os = "macos"))?;
    Some(format!(
        "error: {} is a {foreign_format} archive — built for the other platform\n\
         help: target/ is bind-mount shared host<->devcontainer (ADR 2.7.26b); a build \n\
         help: on the other side clobbered the static library. Rebuild it here:\n\
         help:     cargo build --release -p tungsten_core",
        lib_path.display(),
    ))
}

/// If the static runtime library is missing entirely, return a diagnosis
/// naming the path and how it is resolved. Unlike the format check this is
/// not fail-open — the link command references the archive by absolute path,
/// so a missing file is a guaranteed failure that `cc` would otherwise report
/// as an opaque "linker failed with status 1".
///
/// The classic trigger: `lib_dir` is derived from the running executable's
/// directory, which under `cargo test` is `target/<profile>/deps/` — a
/// location cargo populates only with hashed artifacts, never the unhashed
/// `libtungsten_core.a`.
pub(super) fn missing_staticlib_diagnosis(lib_path: &Path) -> Option<String> {
    if lib_path.exists() {
        return None;
    }
    Some(format!(
        "error: libtungsten_core.a not found at {}\n\
         help: the linker resolves it next to the running executable; build it with:\n\
         help:     cargo build -p tungsten_core\n\
         help: under `cargo test` the executable runs from target/<profile>/deps/, which\n\
         help: lacks the unhashed archive — mirror it there first (see\n\
         help: bootstrap/src/compile/tests/mono_depot_externs.rs::ensure_runtime_staticlib_beside_test_exe)",
        lib_path.display(),
    ))
}

/// Classify the archive's first-member name against the host platform.
///
/// - GNU/SysV archives (Linux toolchains) put the symbol table in a member
///   literally named `/` (or `//` for the extended-name table).
/// - BSD archives (macOS toolchains) start with `__.SYMDEF…` or use the
///   `#1/<len>` extended-name encoding for it.
///
/// Neither signature is ever produced by the other platform's default
/// toolchain, so a match is a definite mismatch; everything else is `None`.
fn foreign_format_name(archive_prefix: &[u8], host_is_macos: bool) -> Option<&'static str> {
    if !archive_prefix.starts_with(AR_MAGIC) || archive_prefix.len() < FIRST_MEMBER_NAME_RANGE.end {
        return None;
    }
    let name_field = &archive_prefix[FIRST_MEMBER_NAME_RANGE];
    let first_member_name = std::str::from_utf8(name_field).ok()?.trim_end();
    let is_gnu_symbol_table = first_member_name == "/" || first_member_name == "//";
    let is_bsd_symbol_table =
        first_member_name.starts_with("__.SYMDEF") || first_member_name.starts_with("#1/");
    if host_is_macos && is_gnu_symbol_table {
        Some("GNU-format (Linux)")
    } else if !host_is_macos && is_bsd_symbol_table {
        Some("BSD-format (macOS)")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a fake archive prefix: magic + a 16-byte member-name field
    /// padded with spaces (the ar header layout).
    fn archive_prefix(first_member_name: &str) -> Vec<u8> {
        let mut bytes = AR_MAGIC.to_vec();
        bytes.extend(format!("{first_member_name:<16}").into_bytes());
        bytes
    }

    #[test]
    fn gnu_symbol_table_is_foreign_on_macos_only() {
        let gnu = archive_prefix("/");
        assert_eq!(foreign_format_name(&gnu, true), Some("GNU-format (Linux)"));
        assert_eq!(foreign_format_name(&gnu, false), None);
        let gnu_extended = archive_prefix("//");
        assert_eq!(
            foreign_format_name(&gnu_extended, true),
            Some("GNU-format (Linux)")
        );
    }

    #[test]
    fn bsd_symbol_table_is_foreign_on_linux_only() {
        for name in ["__.SYMDEF", "__.SYMDEF SORTED", "#1/20"] {
            let bsd = archive_prefix(name);
            assert_eq!(
                foreign_format_name(&bsd, false),
                Some("BSD-format (macOS)"),
                "first member {name:?}"
            );
            assert_eq!(
                foreign_format_name(&bsd, true),
                None,
                "first member {name:?}"
            );
        }
    }

    #[test]
    fn ordinary_member_names_and_non_archives_pass_through() {
        assert_eq!(foreign_format_name(&archive_prefix("foo.o"), true), None);
        assert_eq!(foreign_format_name(&archive_prefix("foo.o"), false), None);
        assert_eq!(foreign_format_name(b"\x7fELF garbage here....", true), None);
        assert_eq!(foreign_format_name(b"!<arch>\n", true), None); // truncated
        assert_eq!(foreign_format_name(b"", false), None);
    }

    #[test]
    fn diagnosis_names_the_fix_and_the_adr() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("libtungsten_core.a");
        std::fs::write(&lib, archive_prefix("/")).unwrap();
        if cfg!(target_os = "macos") {
            let diagnosis = foreign_platform_diagnosis(&lib).unwrap();
            assert!(diagnosis.contains("2.7.26b"), "{diagnosis}");
            assert!(
                diagnosis.contains("cargo build --release -p tungsten_core"),
                "{diagnosis}"
            );
        } else {
            assert_eq!(foreign_platform_diagnosis(&lib), None);
        }
    }

    #[test]
    fn missing_file_fails_open() {
        assert_eq!(
            foreign_platform_diagnosis(Path::new("/nonexistent/libtungsten_core.a")),
            None
        );
    }

    #[test]
    fn missing_staticlib_is_diagnosed_by_name() {
        let missing = Path::new("/nonexistent/libtungsten_core.a");
        let diagnosis = missing_staticlib_diagnosis(missing).unwrap();
        assert!(
            diagnosis.contains("/nonexistent/libtungsten_core.a"),
            "{diagnosis}"
        );
        assert!(
            diagnosis.contains("cargo build -p tungsten_core"),
            "{diagnosis}"
        );
        assert!(diagnosis.contains("deps/"), "{diagnosis}");
    }

    #[test]
    fn present_staticlib_passes_presence_check() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("libtungsten_core.a");
        std::fs::write(&lib, AR_MAGIC).unwrap();
        assert_eq!(missing_staticlib_diagnosis(&lib), None);
    }
}
