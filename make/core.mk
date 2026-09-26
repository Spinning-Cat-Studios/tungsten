# make/core.mk — Core build, test, install, clean targets
#
# Fundamental development commands for building and testing the bootstrap compiler.

.PHONY: build release test test-verbose install reinstall clean clean-artifacts clean-cache fmt lint ci

# Build the compiler
build:
	$(CARGO) build

# Build in release mode
release:
	$(CARGO) build --release

# Run all tests.
## The `build -p tungsten_core` line first (ADR 18.9.26b): the spawned
## `tungsten` links the UNHASHED `target/<profile>/libtungsten_core.a`, which
## only a direct build of the crate uplifts — `cargo test --all` refreshes the
## hashed `deps/` archive and leaves the unhashed one stale, so a new `tg_*`
## symbol fails the diff-exec e2e test at the linker. A no-op when nothing
## changed; the two invocations do not invalidate each other.
test:
	$(CARGO) build -p tungsten_core
	$(CARGO) test --all

# Run compile module tests (requires codegen feature)
test-codegen:
	$(CARGO) build -p tungsten_core
	$(CARGO) test -p tungsten_bootstrap --features codegen --bin tungsten

# Run tests with output
test-verbose:
	$(CARGO) build -p tungsten_core
	$(CARGO) test --all -- --nocapture

# Install the tungsten binary
install:
	$(CARGO) install --path bootstrap --no-default-features

# Reinstall (force)
reinstall:
	$(CARGO) install --path bootstrap --no-default-features --force

# Clean build artifacts
clean: clean-artifacts
	$(CARGO) clean

# Remove generated artifacts (LLVM IR, objects, binaries, logs) without touching cargo cache
clean-artifacts: clean-cache
	@echo "Removing generated artifacts..."
	find . -name '*.ll' -not -path './target/*' -not -path './.git/*' -delete
	find . -name '*.o'  -not -path './target/*' -not -path './.git/*' -delete
	find . -name '*.s'  -not -path './target/*' -not -path './.git/*' -delete
	rm -f *.log
	rm -f hello tungsten1 tungsten1_* tungsten2* tungsten3*
	rm -f libtungsten_core.so libtungsten_core.dylib
	rm -f golden_results.txt runtime_shim.o
	@echo "✓ Artifacts cleaned"

# Remove all elaboration caches (.tungsten/cache/elab/) across the workspace
clean-cache:
	@echo "Removing elaboration caches..."
	find . -path '*/.tungsten/cache/elab' -type d -exec rm -rf {} + 2>/dev/null || true
	@echo "✓ Elab caches cleaned"

# fmt and lint are defined in quality.mk with full options.
# ci uses the quality.mk versions.

# Full CI check
ci: fmt lint test check-examples
	@echo "✓ All CI checks passed"

HELP_SECTIONS += core

# Help section for core commands
.PHONY: help-core
help-core:
	@echo ""
	@echo "  make build        - Build the compiler"
	@echo "  make test         - Run all tests"
	@echo "  make test-codegen - Run compile module tests (codegen feature)"
	@echo "  make install      - Install tungsten binary to ~/.cargo/bin"
	@echo "  make clean           - Full clean (cargo + artifacts)"
	@echo "  make clean-artifacts - Remove .ll, .o, .s, logs, binaries (keeps cargo cache)"
	@echo "  make clean-cache     - Remove elaboration caches (.tungsten/cache/elab/)"
