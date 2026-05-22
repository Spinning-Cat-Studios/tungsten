#!/bin/bash
set -e

# Ensure unlimited stack for deep recursive elaboration/codegen
ulimit -s unlimited

# x86_64 devcontainer post-create script.
# Identical to the default post-create.sh but runs under QEMU emulation
# on ARM Mac hosts. Builds will be slower but produce real x86_64 binaries.

# Ensure log capture directory exists (bind-mounted from host .devcontainer/logs/)
mkdir -p /var/log/tungsten

echo "Installing LLVM 18 (x86_64)..."

# Add LLVM apt repository
wget -qO- https://apt.llvm.org/llvm-snapshot.gpg.key | sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc
echo "deb http://apt.llvm.org/bookworm/ llvm-toolchain-bookworm-18 main" | sudo tee /etc/apt/sources.list.d/llvm.list

# Update package lists
sudo apt-get update

# Install hyperfine for benchmarking
sudo apt-get install -y hyperfine

# Install perf for hardware counter profiling (x86_64 has proper perf support)
# Also install GNU time for memory profiling (time -v)
sudo apt-get install -y linux-perf time || {
    echo "Warning: linux-perf not available for this kernel version."
    echo "Falling back to perf from linux-base."
    sudo apt-get install -y linux-base time || true
}

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

# Create symlinks for LLVM tools
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
echo "Architecture: $(uname -m)"
echo "Hyperfine version: $(hyperfine --version)"

echo "Building Tungsten with codegen (x86_64)..."
CARGO_TARGET_DIR=/tmp/target_x86 cargo build --release

echo "✅ x86_64 dev container setup complete (tests run via make devcontainer-self-compile-verify-x86)"
