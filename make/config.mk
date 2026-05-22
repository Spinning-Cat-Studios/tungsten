# make/config.mk — Shared build configuration
#
# Variables shared across all .mk files. Included first by the root Makefile.

# LLVM 18 paths (native Mac via Homebrew)
LLVM_PREFIX := $(shell brew --prefix llvm@18 2>/dev/null || echo "/opt/homebrew/opt/llvm@18")
LLC := $(LLVM_PREFIX)/bin/llc
OPT := $(LLVM_PREFIX)/bin/opt
export LLVM_SYS_180_PREFIX := $(LLVM_PREFIX)

# Self-hosted compiler source
COMPILER_MAIN := src/compiler/main.tg

# Default max errors for self-hosted checks
MAX_ERRORS := 0
