#!/usr/bin/env bash
set -euo pipefail

# Stage Linux release binaries, fix rpath, and include Docker tooling.
#
# Usage: package-linux.sh <tungsten> <shared-lib> <output-dir>
#
# Copies the binary and .so into <output-dir>, sets rpath to $ORIGIN,
# and includes the Dockerfile + tungsten-docker.sh.

if [ $# -ne 3 ]; then
    echo "Usage: $(basename "$0") <tungsten> <shared-lib> <output-dir>" >&2
    exit 1
fi

TUNGSTEN="$1"
DYLIB="$2"
OUT="$3"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

mkdir -p "$OUT"
cp "$TUNGSTEN" "$OUT/"
cp "$DYLIB" "$OUT/"
cp "$REPO_ROOT/Dockerfile" "$REPO_ROOT/tungsten-docker.sh" "$OUT/"
chmod +x "$OUT/tungsten-docker.sh"

# Fix rpath so the binary finds libtungsten_core.so next to itself
patchelf --set-rpath '$ORIGIN' "$OUT/tungsten"

echo "--- rpath after fix ---"
patchelf --print-rpath "$OUT/tungsten"
