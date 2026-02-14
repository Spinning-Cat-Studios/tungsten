#!/bin/bash
set -e

echo "Installing LLVM 18..."

# Add LLVM apt repository
wget -qO- https://apt.llvm.org/llvm-snapshot.gpg.key | sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc
echo "deb http://apt.llvm.org/bookworm/ llvm-toolchain-bookworm-18 main" | sudo tee /etc/apt/sources.list.d/llvm.list

# Update package lists
sudo apt-get update

echo "Installing Valgrind..."
sudo apt-get install -y \
  valgrind \
  valgrind-dbg \
  gdb

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

echo "Building Tungsten with codegen..."
cargo build --release

echo "Running tests..."
cargo test

echo "✅ Dev container setup complete!"
