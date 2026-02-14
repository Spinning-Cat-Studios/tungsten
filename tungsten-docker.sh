#!/usr/bin/env bash
set -euo pipefail

# Manage and run the Tungsten compiler inside a Docker container.
# Works on any host (including macOS / Apple Silicon) with Docker installed.
#
# Usage:
#   tungsten-docker.sh build                     Build the Docker image
#   tungsten-docker.sh run <args>                Run tungsten <args> in container
#   tungsten-docker.sh exec <cmd...>             Run an arbitrary command in container
#   tungsten-docker.sh shell                     Open an interactive shell
#   tungsten-docker.sh --help                    Show this help message

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLATFORM="linux/amd64"

# Tag the image with a hash of the binary so it auto-invalidates on upgrade.
if [ -f "$SCRIPT_DIR/tungsten" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
        HASH=$(sha256sum "$SCRIPT_DIR/tungsten" | cut -c1-12)
    else
        HASH=$(shasum -a 256 "$SCRIPT_DIR/tungsten" | cut -c1-12)
    fi
    IMAGE_NAME="tungsten:$HASH"
else
    IMAGE_NAME="tungsten:local"
fi

usage() {
    cat <<'EOF'
Tungsten Docker — run the Tungsten compiler in a container

Usage:
  tungsten-docker.sh build                Build the Docker image from Dockerfile
  tungsten-docker.sh run <args>           Run tungsten <args> inside the container
  tungsten-docker.sh exec <cmd...>        Run an arbitrary command in the container
  tungsten-docker.sh shell                Open an interactive shell in the container
  tungsten-docker.sh --help               Show this help message

Examples:
  tungsten-docker.sh build
  tungsten-docker.sh run check myfile.tg
  tungsten-docker.sh run compile hello.tg -o hello
  tungsten-docker.sh run run examples/hello.tg
  tungsten-docker.sh exec ./hello           Run a compiled binary inside the container
  tungsten-docker.sh shell

The Docker image is built automatically on first 'run' or 'shell' if it
does not already exist, and is rebuilt when the tungsten binary changes.
Use 'build' to force a rebuild.

The current directory is mounted at /work inside the container, so .tg
files and compiled output are accessible from both sides.
EOF
}

image_exists() {
    docker image inspect "$IMAGE_NAME" >/dev/null 2>&1
}

ensure_image() {
    if ! image_exists; then
        echo "Building Docker image '$IMAGE_NAME'..."
        build_image
    fi
}

build_image() {
    docker build --platform="$PLATFORM" -t "$IMAGE_NAME" "$SCRIPT_DIR"
}

# Use -it when stdin is a terminal, -i only otherwise (for CI/pipes).
if [ -t 0 ]; then
    TTY_FLAGS="-it"
else
    TTY_FLAGS="-i"
fi

case "${1:-}" in
    build)
        build_image
        echo "Image '$IMAGE_NAME' built successfully."
        ;;
    run)
        shift
        if [ $# -eq 0 ]; then
            echo "Error: 'run' requires arguments. Try: $(basename "$0") run check myfile.tg" >&2
            exit 1
        fi
        ensure_image
        exec docker run --rm $TTY_FLAGS --platform="$PLATFORM" \
            -v "$PWD:/work" -w /work \
            "$IMAGE_NAME" /app/tungsten "$@"
        ;;
    exec)
        shift
        if [ $# -eq 0 ]; then
            echo "Error: 'exec' requires a command. Try: $(basename "$0") exec ./hello" >&2
            exit 1
        fi
        ensure_image
        exec docker run --rm $TTY_FLAGS --platform="$PLATFORM" \
            -v "$PWD:/work" -w /work \
            "$IMAGE_NAME" "$@"
        ;;
    shell)
        ensure_image
        exec docker run --rm $TTY_FLAGS --platform="$PLATFORM" \
            -v "$PWD:/work" -w /work \
            "$IMAGE_NAME" bash
        ;;
    --help|-h|help)
        usage
        ;;
    "")
        usage
        ;;
    *)
        echo "Unknown command: $1" >&2
        echo "Run '$(basename "$0") --help' for usage." >&2
        exit 1
        ;;
esac
