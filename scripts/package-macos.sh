#!/usr/bin/env bash
set -euo pipefail

# Stage macOS release binaries for distribution.
#
# Usage: package-macos.sh <tungsten> <bootstrap> <output-dir>
#
# Copies the two statically-linked binaries and runtime library into <output-dir>.

if [ $# -ne 3 ]; then
    echo "Usage: $(basename "$0") <tungsten> <bootstrap> <output-dir>" >&2
    exit 1
fi

TUNGSTEN="$1"
BOOTSTRAP="$2"
OUT="$3"

# libtungsten_core.a lives next to the bootstrap binary (same directory).
BOOTSTRAP_DIR="$(dirname "$BOOTSTRAP")"
STATIC_LIB="$BOOTSTRAP_DIR/libtungsten_core.a"
if [ ! -f "$STATIC_LIB" ]; then
    echo "error: $STATIC_LIB not found (needed for linking compiled programs)" >&2
    exit 1
fi

mkdir -p "$OUT"
cp "$TUNGSTEN" "$OUT/tungsten"
cp "$BOOTSTRAP" "$OUT/tungsten-bootstrap"
cp "$STATIC_LIB" "$OUT/"

echo "✓ Packaged tungsten + tungsten-bootstrap + libtungsten_core.a into $OUT"
