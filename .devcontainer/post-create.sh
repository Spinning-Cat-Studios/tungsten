#!/bin/bash
set -e

# ADR 4.8.26c D5: this script is the ONE source of truth for LLVM provisioning,
# used both as the devcontainer's postCreateCommand and as a RUN layer in
# `tools/worktree-parallelism-orchestrator/lane.Dockerfile`. The two contexts
# differ in what EXISTS when it runs: at image-build time there is no mounted
# workspace and no named volume, so the steps that touch either must be skipped.
# Set TUNGSTEN_POSTCREATE_PROVISION_ONLY=1 to install the toolchain and stop.
#
# The alternative — a second, forked provisioning script for lanes — is exactly
# the drift D5 exists to prevent: the lane image would quietly stop matching the
# devcontainer, and `lane doctor`'s hash check would be comparing against a file
# nobody edits.
PROVISION_ONLY="${TUNGSTEN_POSTCREATE_PROVISION_ONLY:-0}"

# Ensure unlimited stack for deep recursive elaboration/codegen.
# Tolerated failure: `docker build` layers can run under a hard limit this
# cannot raise, and provisioning apt packages does not need it.
ulimit -s unlimited || true

echo "Installing LLVM 18..."

# Ensure log capture directory exists (bind-mounted from host .devcontainer/logs/)
mkdir -p /var/log/tungsten

# ADR 24.7.26b: the isolated Cargo target volume (CARGO_TARGET_DIR=/build/target,
# from containerEnv) mounts root-owned on a fresh named volume, but the container
# runs as `vscode`; the first `cargo build` would fail EACCES. Give `vscode`
# ownership before any build. (/tmp is world-writable, so the x86_64 container's
# /tmp/target_x86 never needed this — the named volume is the one added cost.)
if [ "$PROVISION_ONLY" != "1" ]; then
  sudo chown vscode:vscode /build/target
fi

# Add LLVM apt repository
wget -qO- https://apt.llvm.org/llvm-snapshot.gpg.key | sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc
echo "deb http://apt.llvm.org/bookworm/ llvm-toolchain-bookworm-18 main" | sudo tee /etc/apt/sources.list.d/llvm.list

# Update package lists
sudo apt-get update

echo "Installing Valgrind, GDB, heaptrack, hyperfine, linux-perf, time, bc..."
# `time` and `bc` are absent from the base image and are what an ad-hoc timing
# loop reaches for first (`/usr/bin/time -f`, `echo "$e - $s" | bc`). Without
# them a hand-rolled measurement fails twice before landing on `date +%s%3N`
# arithmetic — cheap to install, and the ADR 5.8.26c P0 measurement paid that
# toll. hyperfine covers the repeatable case; these cover the one-off.
sudo apt-get install -y \
  valgrind \
  valgrind-dbg \
  gdb \
  heaptrack \
  hyperfine \
  linux-perf \
  time \
  bc

# Install LLVM 18 with all components needed for llvm-sys/inkwell
sudo apt-get install -y \
    llvm-18 \
    llvm-18-dev \
    llvm-18-runtime \
    llvm-18-tools \
    libllvm18 \
    libpolly-18-dev \
    clang-18 \
    lld-18 \
    libclang-18-dev \
    zlib1g-dev \
    libzstd-dev \
    build-essential

# Create symlinks for LLVM tools (llc, opt, llvm-as, etc.)
sudo update-alternatives --install /usr/bin/llc llc /usr/lib/llvm-18/bin/llc 100
sudo update-alternatives --install /usr/bin/opt opt /usr/lib/llvm-18/bin/opt 100
sudo update-alternatives --install /usr/bin/llvm-as llvm-as /usr/lib/llvm-18/bin/llvm-as 100
sudo update-alternatives --install /usr/bin/llvm-dis llvm-dis /usr/lib/llvm-18/bin/llvm-dis 100
sudo update-alternatives --install /usr/bin/clang clang /usr/lib/llvm-18/bin/clang 100
sudo update-alternatives --install /usr/bin/clang++ clang++ /usr/lib/llvm-18/bin/clang++ 100
sudo update-alternatives --install /usr/bin/llvm-link llvm-link /usr/lib/llvm-18/bin/llvm-link 100

# Verify installation
echo "LLVM version:"
/usr/lib/llvm-18/bin/llvm-config --version

echo "Valgrind version:"
valgrind --version

echo "Heaptrack version:"
heaptrack --version || echo "(heaptrack installed)"

echo "Hyperfine version:"
hyperfine --version

echo "perf version (used by make devcontainer-profile-selfcompiled):"
perf --version || echo "(perf installed; userspace/kernel version skew warnings are harmless)"

# Cross-toolchain stale-artifact guard (ADR 17.7.26f): after a container
# recreate whose toolchain sync moved to a new rustc snapshot, the
# bind-mounted target/ can hold rmeta from the previous snapshot, and cargo
# fails with E0460 ("found possibly newer version of crate") / E0463.
# Detect that specific failure, clear the stale artifacts, and retry once;
# any other failure still fails post-create.
cargo_with_stale_artifact_retry() {
  local log=/tmp/post-create-cargo.log
  if ! ("$@" 2>&1 | tee "$log"; exit "${PIPESTATUS[0]}"); then
    if grep -qE "E0460|E0463" "$log"; then
      echo "Stale cross-toolchain artifacts detected (E0460/E0463) — running cargo clean and retrying once..."
      cargo clean
      "$@"
    else
      return 1
    fi
  fi
}

if [ "$PROVISION_ONLY" = "1" ]; then
  # Image-build context: no workspace is mounted, so there is nothing to build
  # or test. A lane's first build happens inside the lane, against its own
  # /build volume.
  echo "✅ Toolchain provisioning complete (TUNGSTEN_POSTCREATE_PROVISION_ONLY=1)."
  exit 0
fi

echo "Building Tungsten with codegen..."
cargo_with_stale_artifact_retry cargo build --release

echo "Running tests..."
cargo_with_stale_artifact_retry cargo test

echo "✅ Dev container setup complete!"
