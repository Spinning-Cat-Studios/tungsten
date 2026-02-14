# Tungsten Makefile
# ==================
#
# Common commands for working with the Tungsten project.

.PHONY: build test install clean check run eval help run-examples check-examples check-golden update-golden devcontainer-up devcontainer-test devcontainer-down devcontainer-ensure-lib-symlink devcontainer-self-compile-step2 devcontainer-full-bootstrap devcontainer-check-l4 ensure-lib-symlink self-compile-step2 full-bootstrap

# Default target
help:
	@echo "Tungsten Development Commands"
	@echo "=============================="
	@echo ""
	@echo "  make build     - Build the compiler"
	@echo "  make test      - Run all tests"
	@echo "  make install   - Install tungsten binary to ~/.cargo/bin"
	@echo "  make clean     - Remove build artifacts"
	@echo ""
	@echo "  make check FILE=<file>  - Type-check a file"
	@echo "  make run FILE=<file>    - Run a file (interpreted)"
	@echo "  make eval EXPR=<expr>   - Evaluate an expression"
	@echo ""
	@echo "Self-Hosted Compiler:"
	@echo "  make check-compiler  - Type-check the self-hosted compiler"
	@echo "  make check-lexer     - Type-check lexer module only"
	@echo "  make check-modules   - Run module integration tests"
	@echo ""
	@echo "Native Compilation (requires LLVM 18):"
	@echo "  make compile FILE=<file>      - Compile to native binary"
	@echo "  make compile-run FILE=<file>  - Compile and run"
	@echo ""
	@echo "Native Mac Self-Compile (brew install llvm@18):"
	@echo "  make self-compile             - Build tungsten1 (IR→object→link)"
	@echo "  make self-compile-fast        - Build tungsten1 with -O0 (faster)"
	@echo "  make self-compile-ir          - Generate tungsten1.ll only"
	@echo "  make self-compile-llc         - Compile tungsten1.ll → .o (with stats)"
	@echo "  make self-compile-llc-fast    - Compile tungsten1.ll → .o (-O0)"
	@echo "  make self-compile-link        - Link tungsten1.o → tungsten1"
	@echo "  make self-compile-step2       - Step 2: tungsten1 → tungsten2"
	@echo "  make full-bootstrap           - Full bootstrap (tungsten1 + tungsten2)"
	@echo ""
	@echo "Dev Container (LLVM codegen):"
	@echo "  make devcontainer-up       - Start dev container with LLVM 18"
	@echo "  make devcontainer-test     - Run codegen tests in container"
	@echo "  make devcontainer-test-all - Run all tests in container"
	@echo "  make devcontainer-build    - Build with codegen in container"
	@echo "  make devcontainer-compile FILE=<file>     - Compile to native"
	@echo "  make devcontainer-compile-run FILE=<file> - Compile and run"
	@echo "  make devcontainer-run FILE=<file>         - Run (interpreted)"
	@echo "  make devcontainer-check FILE=<file>       - Type-check a file"
	@echo "  make devcontainer-eval EXPR=<expr>        - Eval an expression"
	@echo "  make devcontainer-check-l2                - Check L2 compiler for errors"
	@echo "  make devcontainer-build-check-l2          - Build then check L2"
	@echo "  make devcontainer-check-l3                - Check L3 (bootstrap checks self-hosted)"
	@echo "  make devcontainer-build-check-l3          - Build bootstrap then check L3"
	@echo "  make devcontainer-self-compile-step2      - Build tungsten2 from tungsten1"
	@echo "  make devcontainer-full-bootstrap          - Full bootstrap (tungsten1 + tungsten2)"
	@echo "  make devcontainer-check-l4                - Check L4 (tungsten2 checks itself)"
	@echo "  make devcontainer-down                    - Stop dev container"
	@echo ""
	@echo "Examples:"
	@echo "  make run FILE=examples/hello.tg"
	@echo "  make eval EXPR='2 + 2'"
	@echo "  make devcontainer-up && make devcontainer-test"

# Build the compiler
build:
	cargo build

# Build in release mode
release:
	cargo build --release

# Run all tests
test:
	cargo test --all

# Run tests with output
test-verbose:
	cargo test --all -- --nocapture

# Install the tungsten binary
install:
	cargo install --path bootstrap --no-default-features

# Reinstall (force)
reinstall:
	cargo install --path bootstrap --no-default-features --force

# Clean build artifacts
clean:
	cargo clean

# Type-check a file (no LLVM needed)
check:
ifndef FILE
	@echo "Usage: make check FILE=<file>"
	@echo "Example: make check FILE=examples/hello.tg"
else
	cargo run -p tungsten_bootstrap --no-default-features -- check $(FILE)
endif

# Run a file (no LLVM needed)
run:
ifndef FILE
	@echo "Usage: make run FILE=<file>"
	@echo "Example: make run FILE=examples/hello.tg"
else
	cargo run -p tungsten_bootstrap --no-default-features -- $(FILE)
endif

# Evaluate an expression (no LLVM needed)
eval:
ifndef EXPR
	@echo "Usage: make eval EXPR=<expr>"
	@echo "Example: make eval EXPR='2 + 2'"
else
	cargo run -p tungsten_bootstrap --no-default-features -- eval "$(EXPR)"
endif

# =============================================================================
# Native Compilation (requires LLVM 18)
# =============================================================================

# Compile to native binary (requires LLVM 18 installed locally)
compile:
ifndef FILE
	@echo "Usage: make compile FILE=<file> [OUT=<output>]"
	@echo "Example: make compile FILE=examples/hello.tg OUT=hello"
	@echo ""
	@echo "Note: Requires LLVM 18. Set LLVM_SYS_180_PREFIX or use devcontainer."
else
	cargo run -p tungsten_bootstrap --release -- compile $(FILE) $(if $(OUT),-o $(OUT),)
endif

# Compile and run a native binary (requires LLVM 18)
# Supports: make compile-run FILE=examples/hello.tg  OR  make compile-run examples/hello.tg
compile-run:
	$(eval _FILE := $(or $(FILE),$(filter %.tg,$(MAKECMDGOALS))))
	@if [ -z "$(_FILE)" ]; then \
		echo "Usage: make compile-run FILE=<file>"; \
		echo "   or: make compile-run <file>"; \
		echo "Example: make compile-run examples/hello.tg"; \
		echo ""; \
		echo "Note: Requires LLVM 18. Set LLVM_SYS_180_PREFIX or use devcontainer."; \
		exit 1; \
	fi && \
	BASENAME=$$(basename $(_FILE) .tg) && \
	cargo run -p tungsten_bootstrap --release -- compile $(_FILE) -o /tmp/$$BASENAME && \
	echo "Running /tmp/$$BASENAME..." && \
	/tmp/$$BASENAME

# Run all examples
run-examples:
	@echo "=== hello.tg ===" && cargo run -p tungsten_bootstrap -- examples/hello.tg
	@echo "=== answer.tg ===" && cargo run -p tungsten_bootstrap -- examples/answer.tg
	@echo "=== arithmetic.tg ===" && cargo run -p tungsten_bootstrap -- examples/arithmetic.tg
	@echo "=== logic.tg ===" && cargo run -p tungsten_bootstrap -- examples/logic.tg
	@echo "=== proof.tg ===" && cargo run -p tungsten_bootstrap -- examples/proof.tg

# Check all examples type-check
check-examples:
	@cargo run -p tungsten_bootstrap -- check examples/hello.tg
	@cargo run -p tungsten_bootstrap -- check examples/answer.tg
	@cargo run -p tungsten_bootstrap -- check examples/arithmetic.tg
	@cargo run -p tungsten_bootstrap -- check examples/logic.tg
	@cargo run -p tungsten_bootstrap -- check examples/proof.tg

# Run golden tests (compare output with expected)
check-golden:
	@./tests/golden/run_golden.sh

# Update golden test expected files from bootstrap output
update-golden:
	@./tests/golden/run_golden.sh --update

# Format code (if rustfmt is available)
fmt:
	cargo fmt --all

# Lint code
lint:
	cargo clippy --all -- -D warnings

# Full CI check
ci: fmt lint test check-examples
	@echo "✓ All CI checks passed"

# =============================================================================
# Self-Hosted Compiler Checks
# =============================================================================

# Check that the self-hosted compiler type-checks
check-compiler:
	@echo "Checking self-hosted compiler..."
	@rm -rf src/compiler/.tungsten src/compiler/**/.tungsten
	@cargo run -p tungsten_bootstrap --no-default-features -- check src/compiler/main.tg
	@echo "✓ Self-hosted compiler type-checks successfully"

# Check lexer module only
check-lexer:
	@echo "Checking lexer module..."
	@rm -rf src/compiler/.tungsten src/compiler/**/.tungsten
	@cargo run -p tungsten_bootstrap --no-default-features -- check src/compiler/lexer/mod.tg
	@echo "✓ Lexer module type-checks successfully"

# Check parser module only (requires main.tg context for lexer access)
check-parser:
	@echo "Checking parser module..."
	@rm -rf src/compiler/.tungsten src/compiler/**/.tungsten
	@cargo run -p tungsten_bootstrap --no-default-features -- check src/compiler/parser/mod.tg
	@echo "✓ Parser module type-checks successfully"

# Check module system integration tests
check-modules:
	@echo "Checking module integration tests..."
	@cargo run -p tungsten_bootstrap --no-default-features -- check tests/module_bugs/lexer_parser_pattern/main.tg
	@echo "✓ Module integration tests pass"

# =============================================================================
# Native Mac LLVM 18 (faster than devcontainer)
# =============================================================================
# Requires: brew install llvm@18

LLVM_PREFIX := $(shell brew --prefix llvm@18 2>/dev/null || echo "/opt/homebrew/opt/llvm@18")
LLC := $(LLVM_PREFIX)/bin/llc
OPT := $(LLVM_PREFIX)/bin/opt
export LLVM_SYS_180_PREFIX := $(LLVM_PREFIX)

# Full self-compile: IR → object → binary (native Mac)
self-compile:
	@echo "=== Stage 0/3: Building bootstrap compiler and FFI library ==="
	cargo build --release -p tungsten_bootstrap -p tungsten_core -p tungsten_codegen
	@echo ""
	@echo "=== Stage 1/3: Generating LLVM IR ==="
	./target/release/tungsten compile src/compiler/main.tg --emit-llvm -o tungsten1.ll -v
	@echo ""
	@echo "=== Stage 2/3: Compiling IR to object file (llc) ==="
	@echo "Using $(LLC)"
	$(LLC) -filetype=obj tungsten1.ll -o tungsten1.o -O2 --stats
	@echo ""
	@echo "=== Stage 3/3: Linking ==="
	cc tungsten1.o -o tungsten1 -L target/release -ltungsten_core
	rm -f tungsten1.ll tungsten1.o
	@echo ""
	@echo "✓ Built tungsten1"

# Fast self-compile with -O0 (for testing, ~10-20x faster llc)
self-compile-fast:
	@echo "=== Stage 0/3: Building bootstrap compiler and FFI library ==="
	cargo build --release -p tungsten_bootstrap -p tungsten_core -p tungsten_codegen
	@echo ""
	@echo "=== Stage 1/3: Generating LLVM IR ==="
	./target/release/tungsten compile src/compiler/main.tg --emit-llvm -o tungsten1.ll -v
	@echo ""
	@echo "=== Stage 2/3: Compiling IR to object file (llc -O0) ==="
	@echo "Using $(LLC) with -O0 for speed"
	$(LLC) -filetype=obj tungsten1.ll -o tungsten1.o -O0
	@echo ""
	@echo "=== Stage 3/3: Linking ==="
	cc tungsten1.o -o tungsten1 -L target/release -ltungsten_core
	rm -f tungsten1.ll tungsten1.o
	@echo ""
	@echo "✓ Built tungsten1 (unoptimized)"

build-bootstrap:
	cargo build --release -p tungsten_bootstrap -p tungsten_core -p tungsten_codegen

# Ensure libtungsten_core symlink exists for native Mac (for tungsten1 linking)
ensure-lib-symlink:
	@test -L libtungsten_core.dylib || ln -sf target/release/libtungsten_core.dylib .

# Step 2: tungsten1 compiles itself to tungsten2 (native Mac)
self-compile-step2: ensure-lib-symlink
	@if [ ! -f tungsten1 ]; then \
		echo "Error: tungsten1 not found. Run 'make self-compile' first."; \
		exit 1; \
	fi
	DYLD_LIBRARY_PATH=target/release ./tungsten1 compile src/compiler/main.tg -o tungsten2 -v
	@echo "✓ Built tungsten2 (self-hosted compiler compiled by tungsten1)"

# Full bootstrap: Step 1 + Step 2 (native Mac)
full-bootstrap: self-compile self-compile-step2
	@echo ""
	@echo "=== Bootstrap Complete ==="
	@ls -lh tungsten1 tungsten2
	@echo ""
	@echo "To verify L4: DYLD_LIBRARY_PATH=target/release ./tungsten2 check src/compiler/main.tg"

# Generate IR only (native)
self-compile-ir:
	cargo run -p tungsten_bootstrap --release -- compile src/compiler/main.tg --emit-llvm -o tungsten1.ll -v

# Compile IR to object (requires tungsten1.ll)
self-compile-llc:
	@echo "Using $(LLC)"
	$(LLC) -filetype=obj tungsten1.ll -o tungsten1.o -O2 -time-passes --stats

# Compile IR to object with -O0 (fast, requires tungsten1.ll)
self-compile-llc-fast:
	@echo "Using $(LLC) with -O0"
	$(LLC) -filetype=obj tungsten1.ll -o tungsten1.o -O0

# Link object to binary (requires tungsten1.o)
self-compile-link:
	cc tungsten1.o -o tungsten1 -L target/release -ltungsten_core
	@echo "✓ Linked tungsten1"

# =============================================================================
# Dev Container (for LLVM codegen testing)
# =============================================================================
# Requires: npm install -g @devcontainers/cli

# Start the dev container
devcontainer-up:
	devcontainer up --workspace-folder .

# Run codegen tests in dev container
devcontainer-test:
	devcontainer exec --workspace-folder . cargo test -p tungsten_codegen

# Run all tests in dev container
devcontainer-test-all:
	devcontainer exec --workspace-folder . cargo test

# Build with codegen in dev container
devcontainer-build:
	devcontainer exec --workspace-folder . cargo build --release

# Compile to native binary in dev container
devcontainer-compile:
ifndef FILE
	@echo "Usage: make devcontainer-compile FILE=<file> [OUT=<output>]"
	@echo "Example: make devcontainer-compile FILE=examples/hello.tg OUT=hello"
else
	devcontainer exec --workspace-folder . cargo run -p tungsten_bootstrap --release -- compile $(FILE) $(if $(OUT),-o $(OUT),)
endif

# Compile and run a native binary in dev container
# Supports: make devcontainer-compile-run FILE=examples/hello.tg  OR  make devcontainer-compile-run examples/hello.tg
devcontainer-compile-run:
	$(eval _FILE := $(or $(FILE),$(filter %.tg,$(MAKECMDGOALS))))
	@if [ -z "$(_FILE)" ]; then \
		echo "Usage: make devcontainer-compile-run FILE=<file>"; \
		echo "   or: make devcontainer-compile-run <file>"; \
		echo "Example: make devcontainer-compile-run examples/hello.tg"; \
		exit 1; \
	fi && \
	BASENAME=$$(basename $(_FILE) .tg) && \
	devcontainer exec --workspace-folder . bash -c "\
		cargo run -p tungsten_bootstrap --release -- compile $(_FILE) -o ./$$BASENAME && \
		echo 'Running ./$$BASENAME...' && \
		./$$BASENAME && \
		rm -f ./$$BASENAME"

# Run a file in dev container (interpreter)
devcontainer-run:
ifndef FILE
	@echo "Usage: make devcontainer-run FILE=<file>"
	@echo "Example: make devcontainer-run FILE=src/compiler/lexer_all.tg"
else
	devcontainer exec --workspace-folder . cargo run -p tungsten_bootstrap -- run $(FILE)
endif

# Check a file in dev container
devcontainer-check:
ifndef FILE
	@echo "Usage: make devcontainer-check FILE=<file>"
	@echo "Example: make devcontainer-check FILE=src/compiler/lexer_all.tg"
else
	devcontainer exec --workspace-folder . cargo run -p tungsten_bootstrap -- check $(FILE)
endif

# Eval an expression in dev container
devcontainer-eval:
ifndef EXPR
	@echo "Usage: make devcontainer-eval EXPR=<expr>"
	@echo "Example: make devcontainer-eval EXPR='char_at(\"hello\", 0)'"
else
	devcontainer exec --workspace-folder . cargo run -p tungsten_bootstrap -- eval "$(EXPR)"
endif

# Check L2 self-hosted compiler for type errors (uses release build)
devcontainer-check-l2:
	devcontainer exec --workspace-folder . ./target/release/tungsten check src/compiler/main.tg --max-errors=0

# Build and check L2 self-hosted compiler
devcontainer-build-check-l2: devcontainer-build devcontainer-check-l2

# Check L3: bootstrap compiler type-checks the self-hosted source
devcontainer-check-l3:
	devcontainer exec --workspace-folder . cargo run -p tungsten_bootstrap --no-default-features -- check src/compiler/main.tg --max-errors=0

# Build bootstrap and check L3
devcontainer-build-check-l3:
	devcontainer exec --workspace-folder . cargo build -p tungsten_bootstrap --no-default-features
	devcontainer exec --workspace-folder . cargo run -p tungsten_bootstrap --no-default-features -- check src/compiler/main.tg --max-errors=0

# Ensure libtungsten_core.so symlink exists (for binaries in workspace root)
devcontainer-ensure-lib-symlink:
	@devcontainer exec --workspace-folder . bash -c 'test -L libtungsten_core.so || ln -sf target/release/libtungsten_core.so .'

# Compile the self-hosted compiler to native in dev container (Step 1: bootstrap → tungsten1)
devcontainer-self-compile: devcontainer-ensure-lib-symlink
	devcontainer exec --workspace-folder . ./target/release/tungsten compile src/compiler/main.tg -o tungsten1 -v

# Compile the self-hosted compiler + check it (Step 1 + Step 2: tungsten1 → tungsten2)
devcontainer-self-compile-with-check:
	devcontainer-self-compile 2>&1 | tail -3 && docker exec -w /workspaces/Tungsten epic_ramanujan ./tungsten1 check src/compiler/main.tg

# Step 2: tungsten1 compiles itself to tungsten2
devcontainer-self-compile-step2: devcontainer-ensure-lib-symlink
	@if [ ! -f tungsten1 ]; then \
		echo "Error: tungsten1 not found. Run 'make devcontainer-self-compile' first."; \
		exit 1; \
	fi
	devcontainer exec --workspace-folder . ./tungsten1 compile src/compiler/main.tg -o tungsten2 -v
	@echo "✓ Built tungsten2 (self-hosted compiler compiled by tungsten1)"

# Full bootstrap: Step 1 + Step 2
devcontainer-full-bootstrap: devcontainer-self-compile devcontainer-self-compile-step2
	@echo ""
	@echo "=== Bootstrap Complete ==="
	@devcontainer exec --workspace-folder . ls -lh tungsten1 tungsten2
	@echo ""
	@echo "To verify L4: make devcontainer-check-l4"

# L4 verification: tungsten2 checks itself
devcontainer-check-l4:
	devcontainer exec --workspace-folder . ./tungsten2 check src/compiler/main.tg --max-errors=0

# Compile self-hosted compiler with split IR/object/link stages (shows progress)
# Stage 1: Generate LLVM IR (fast, ~2-3 min)
# Stage 2: Compile IR to object with llc (slow, shows stats)
# Stage 3: Link to final binary
devcontainer-self-compile-split:
	@echo "=== Stage 1/3: Generating LLVM IR ==="
	devcontainer exec --workspace-folder . ./target/release/tungsten compile src/compiler/main.tg --emit-llvm -o tungsten1.ll -v
	@echo ""
	@echo "=== Stage 2/3: Compiling IR to object file (llc) ==="
	@echo "This may take several minutes..."
	devcontainer exec --workspace-folder . llc -filetype=obj tungsten1.ll -o tungsten1.o -O2 --stats
	@echo ""
	@echo "=== Stage 3/3: Linking ==="
	devcontainer exec --workspace-folder . cc tungsten1.o -o tungsten1
	devcontainer exec --workspace-folder . rm -f tungsten1.ll tungsten1.o
	@echo ""
	@echo "✓ Built tungsten1"

# Generate IR only (for debugging)
devcontainer-self-compile-ir:
	devcontainer exec --workspace-folder . ./target/release/tungsten compile src/compiler/main.tg --emit-llvm -o tungsten1.ll -v

# Compile IR to object with detailed timing (requires tungsten1.ll exists)
devcontainer-self-compile-llc:
	devcontainer exec --workspace-folder . llc-18 -filetype=obj tungsten1.ll -o tungsten1.o -O2 -time-passes --stats

# Link object to binary (requires tungsten1.o exists)
devcontainer-self-compile-link:
	devcontainer exec --workspace-folder . cc tungsten1.o -o tungsten1
	@echo "✓ Linked tungsten1"

# Stop and remove the dev container
devcontainer-down:
	@CONTAINER_ID=$$(docker ps -q --filter "label=devcontainer.local_folder=$$(pwd)"); \
	if [ -n "$$CONTAINER_ID" ]; then \
		docker stop $$CONTAINER_ID && docker rm $$CONTAINER_ID; \
		echo "Dev container stopped and removed"; \
	else \
		echo "No dev container running"; \
	fi

# Catch-all for .tg files used as positional arguments
%.tg:
	@true
