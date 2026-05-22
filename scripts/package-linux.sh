#!/usr/bin/env bash
set -euo pipefail

# Stage Linux release binaries for distribution.
#
# Usage: package-linux.sh <tungsten> <output-dir>
#
# Copies the statically-linked binary into <output-dir> and includes
# the Dockerfile + tungsten-docker.sh.

if [ $# -ne 2 ]; then
    echo "Usage: $(basename "$0") <tungsten> <output-dir>" >&2
    exit 1
fi

TUNGSTEN="$1"
OUT="$2"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# libtungsten_core.a lives next to the binary (same directory).
TUNGSTEN_DIR="$(dirname "$TUNGSTEN")"
STATIC_LIB="$TUNGSTEN_DIR/libtungsten_core.a"
if [ ! -f "$STATIC_LIB" ]; then
    echo "error: $STATIC_LIB not found (needed for linking compiled programs)" >&2
    exit 1
fi

mkdir -p "$OUT"
cp "$TUNGSTEN" "$OUT/"
cp "$STATIC_LIB" "$OUT/"
cp "$REPO_ROOT/Dockerfile" "$REPO_ROOT/tungsten-docker.sh" "$OUT/"
chmod +x "$OUT/tungsten-docker.sh"

echo "✓ Packaged tungsten + libtungsten_core.a into $OUT"
