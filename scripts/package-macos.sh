#!/usr/bin/env bash
set -euo pipefail

# Stage macOS release binaries and fix dylib paths.
#
# Usage: package-macos.sh <tungsten> <bootstrap> <dylib> <output-dir>
#
# Copies the three binaries into <output-dir> and rewrites dylib load
# paths to use @rpath (no DYLD_LIBRARY_PATH needed).
#
# Compiler binaries get @executable_path added as an rpath so they find
# the dylib next to themselves. Compiled output binaries find it via the
# absolute rpath the compiler embeds at link time (-Wl,-rpath,<lib_dir>).

if [ $# -ne 4 ]; then
    echo "Usage: $(basename "$0") <tungsten> <bootstrap> <dylib> <output-dir>" >&2
    exit 1
fi

TUNGSTEN="$1"
BOOTSTRAP="$2"
DYLIB="$3"
OUT="$4"

mkdir -p "$OUT"
cp "$TUNGSTEN" "$OUT/tungsten"
cp "$BOOTSTRAP" "$OUT/tungsten-bootstrap"
cp "$DYLIB" "$OUT/libtungsten_core.dylib"

# Fix dylib install name to use @rpath
install_name_tool -id @rpath/libtungsten_core.dylib "$OUT/libtungsten_core.dylib"

# Rewrite each binary's reference to the dylib (skip if not dynamically linked)
for bin in "$OUT/tungsten" "$OUT/tungsten-bootstrap"; do
    OLD_PATH=$(otool -L "$bin" | grep libtungsten_core | head -1 | awk '{print $1}' || true)
    if [ -n "$OLD_PATH" ]; then
        install_name_tool -change "$OLD_PATH" @rpath/libtungsten_core.dylib "$bin"
        # Add @executable_path as rpath so the binary finds the dylib next to itself
        if ! otool -l "$bin" | grep -q "@executable_path"; then
            install_name_tool -add_rpath @executable_path "$bin"
        fi
        echo "Fixed: $bin"
    else
        echo "Skipped (no dylib reference): $bin"
    fi
done

echo "--- dylib references after fix ---"
for bin in "$OUT/tungsten" "$OUT/tungsten-bootstrap"; do
    echo "$bin:"
    otool -L "$bin" | grep tungsten_core || echo "  (none)"
done
