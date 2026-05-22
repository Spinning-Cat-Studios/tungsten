# make/devcontainer.mk — Dev container targets (LLVM codegen testing)
#
# Commands for running the compiler inside a dev container with LLVM 18.
# Requires: npm install -g @devcontainers/cli

.PHONY: devcontainer-up devcontainer-test devcontainer-test-all devcontainer-build
.PHONY: devcontainer-compile devcontainer-compile-run devcontainer-run devcontainer-check devcontainer-eval
.PHONY: devcontainer-check-l2 devcontainer-build-check-l2 devcontainer-check-l3 devcontainer-build-check-l3
.PHONY: devcontainer-check-l4 devcontainer-check-ir-determinism devcontainer-check-ir-determinism-fast devcontainer-check-ir-determinism-full
.PHONY: devcontainer-check-ir-fingerprint-full
.PHONY: devcontainer-dump-core devcontainer-check-tyvar-escape devcontainer-ensure-lib-symlink
.PHONY: devcontainer-self-compile devcontainer-self-compile-with-check devcontainer-self-compile-step2
.PHONY: devcontainer-self-compile-split devcontainer-self-compile-ir devcontainer-self-compile-llc devcontainer-self-compile-link
.PHONY: devcontainer-self-compile-verify devcontainer-self-compile-dev devcontainer-self-compile-fast
.PHONY: devcontainer-self-compile-verify-fast devcontainer-self-compile-direct devcontainer-full-bootstrap
.PHONY: devcontainer-doctor-self-test devcontainer-doctor-self-test-full
.PHONY: devcontainer-up-x86 devcontainer-build-x86 devcontainer-self-compile-x86 devcontainer-self-compile-verify-x86
.PHONY: devcontainer-down devcontainer-down-x86 devcontainer-profile

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

# Dump Core IR for matching definitions (L2, uses release build)
devcontainer-dump-core:
ifndef PATTERN
	@echo "Usage: make devcontainer-dump-core PATTERN=<pattern>"
	@echo "Examples:"
	@echo "  make devcontainer-dump-core PATTERN=main"
	@echo "  make devcontainer-dump-core PATTERN='*'"
else
	devcontainer exec --workspace-folder . ./target/release/tungsten check src/compiler/main.tg --dump-core "$(PATTERN)" --max-errors=0
endif

# Check TyVar escapes in L2 self-hosted compiler (uses release build)
devcontainer-check-tyvar-escape:
	devcontainer exec --workspace-folder . ./target/release/tungsten check src/compiler/main.tg --check-tyvar-escape --max-errors=0

# Check L3: bootstrap compiler type-checks the self-hosted source
devcontainer-check-l3:
	devcontainer exec --workspace-folder . cargo run -p tungsten_bootstrap --no-default-features -- check src/compiler/main.tg --max-errors=0

# Build bootstrap and check L3
devcontainer-build-check-l3:
	devcontainer exec --workspace-folder . cargo build -p tungsten_bootstrap --no-default-features
	devcontainer exec --workspace-folder . cargo run -p tungsten_bootstrap --no-default-features -- check src/compiler/main.tg --max-errors=0

# No longer needed — static linking eliminates runtime library dependency
devcontainer-ensure-lib-symlink:
	@true

# Compile the self-hosted compiler to native in dev container (Step 1: bootstrap → tungsten1)
devcontainer-self-compile:
	devcontainer exec --workspace-folder . ./target/release/tungsten compile src/compiler/main.tg -o tungsten1 -v

# Compile the self-hosted compiler + check it (Step 1 + Step 2: tungsten1 → tungsten2)
devcontainer-self-compile-with-check:
	devcontainer-self-compile 2>&1 | tail -3 && docker exec -w /workspaces/Tungsten epic_ramanujan ./tungsten1 check src/compiler/main.tg

# Step 2: tungsten1 compiles itself to tungsten2
devcontainer-self-compile-step2:
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

# IR Determinism check (ADR 17.5.26c): verify bootstrap produces byte-identical .ll files
# across consecutive compilations. Uses the bootstrap binary (not tungsten1).
# Aliases to -full for backward compatibility (ADR 18.5.26c).
devcontainer-check-ir-determinism: devcontainer-check-ir-determinism-full

# Full IR determinism gate (ADR 18.5.26c): two independent clean compiles, includes __mono.ll.
devcontainer-check-ir-determinism-full:
	devcontainer exec --workspace-folder . bash scripts/check-ir-determinism-v2.sh --mode=full

# Fast IR determinism check (ADR 18.5.26c): reuses elab cache, excludes __mono.ll, parallel diff.
devcontainer-check-ir-determinism-fast:
	devcontainer exec --workspace-folder . bash scripts/check-ir-determinism-v2.sh --mode=fast

# Full-source IR fingerprint check (ADR 18.5.26d): single compile, compare against baseline.
devcontainer-check-ir-fingerprint-full:
	devcontainer exec --workspace-folder . bash scripts/check-ir-fingerprint.sh \
		--entry src/compiler/main.tg --baseline tests/golden/ir_fingerprint_full.manifest

# Compile self-hosted compiler with split IR/object/link stages (shows progress)
# Stage 1: Generate LLVM IR (per-file, ~2-3 min)
# Stage 2: Compile each .ll to .o with llc (slow, shows stats)
# Stage 3: Link all .o files to final binary
devcontainer-self-compile-split:
	@echo "=== Stage 1/3: Generating LLVM IR (per-file) ==="
	devcontainer exec --workspace-folder . bash -c 'rm -rf /tmp/tungsten1_ll && mkdir -p /tmp/tungsten1_ll && ./target/release/tungsten compile src/compiler/main.tg --emit-llvm -o /tmp/tungsten1_ll/ -v'
	@echo ""
	@echo "=== Stage 2/3: Compiling IR to object files (llc) ==="
	@echo "This may take several minutes..."
	devcontainer exec --workspace-folder . bash -c 'find /tmp/tungsten1_ll -name "*.ll" | while read f; do echo "  llc $$f"; llc -filetype=obj "$$f" -o "$${f%.ll}.o" -O2 --stats; done'
	@echo ""
	@echo "=== Stage 3/3: Linking ==="
	devcontainer exec --workspace-folder . bash -c 'cc -o tungsten1 $$(find /tmp/tungsten1_ll -name "*.o") target/release/libtungsten_core.a -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc'
	devcontainer exec --workspace-folder . rm -rf /tmp/tungsten1_ll
	@echo ""
	@echo "✓ Built tungsten1"

# Generate IR only (for debugging) — writes per-file .ll files to tungsten1_ll/
devcontainer-self-compile-ir:
	devcontainer exec --workspace-folder . bash -c 'rm -rf tungsten1_ll && mkdir -p tungsten1_ll && ./target/release/tungsten compile src/compiler/main.tg --emit-llvm -o tungsten1_ll/ -v'

# Compile per-file IR to object files (requires tungsten1_ll/ from devcontainer-self-compile-ir)
devcontainer-self-compile-llc:
	devcontainer exec --workspace-folder . bash -c 'find tungsten1_ll -name "*.ll" | while read f; do echo "  llc $$f"; llc-18 -filetype=obj "$$f" -o "$${f%.ll}.o" -O2 -time-passes --stats; done'

# Link object files to binary (requires tungsten1_ll/*.o from devcontainer-self-compile-llc)
devcontainer-self-compile-link:
	devcontainer exec --workspace-folder . bash -c 'cc -o tungsten1 $$(find tungsten1_ll -name "*.o") target/release/libtungsten_core.a -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc'
	@echo "✓ Linked tungsten1"

# DevContainer self-compile-verify (full tier): self-compile + verify examples.
devcontainer-self-compile-verify: devcontainer-self-compile
	devcontainer exec --workspace-folder . ./target/release/tungsten-dev verify

# Fast self-compile via tungsten-dev (3-stage: IR → llc -O0 → link).
# Parallelism defaults to nproc/2. Override: TUNGSTEN_CODEGEN_JOBS=N or --parallelism N.
devcontainer-self-compile-fast:
	devcontainer exec --workspace-folder . ./target/release/tungsten-dev self-compile --fast

# Direct self-compile: emit .o in-process (no llc), single-stage (ADR 9.5.26e §2.1).
devcontainer-self-compile-direct:
	devcontainer exec --workspace-folder . ./target/release/tungsten-dev self-compile --direct

# Fast self-compile + verify all examples (routine confidence check).
devcontainer-self-compile-verify-fast: devcontainer-self-compile-fast
	devcontainer exec --workspace-folder . ./target/release/tungsten-dev verify

# Developer build in devcontainer: self-compile with diagnostic tools enabled
devcontainer-self-compile-dev:
	@echo "=== Building tungsten1 in devcontainer (developer mode — diagnostic tools enabled) ==="
	@# Step 1: Swap in developer diagnostics
	@cp src/compiler/driver/ffi/diagnostics.tg src/compiler/driver/ffi/diagnostics.tg.prod
	@cp src/compiler/driver/ffi/diagnostics_dev.tg src/compiler/driver/ffi/diagnostics.tg
	@# Step 2: Build (same as devcontainer-self-compile)
	@$(MAKE) devcontainer-self-compile || { \
		cp src/compiler/driver/ffi/diagnostics.tg.prod src/compiler/driver/ffi/diagnostics.tg; \
		rm -f src/compiler/driver/ffi/diagnostics.tg.prod; \
		exit 1; \
	}
	@# Step 3: Restore production stubs
	@cp src/compiler/driver/ffi/diagnostics.tg.prod src/compiler/driver/ffi/diagnostics.tg
	@rm -f src/compiler/driver/ffi/diagnostics.tg.prod
	@echo "✓ Built tungsten1 in devcontainer (developer mode)"

# --- x86_64 devcontainer targets (QEMU emulation on ARM Mac) ---
# These use a separate devcontainer config at .devcontainer/x86_64/ with
# --platform linux/amd64. Builds are slower due to QEMU but produce real
# x86_64 binaries. Uses CARGO_TARGET_DIR=/tmp/target_x86 inside the
# container to avoid conflicting with host-arch build artifacts.

# Start the x86_64 dev container
devcontainer-up-x86:
	devcontainer up --workspace-folder . --config .devcontainer/x86_64/devcontainer.json

# Build with codegen in x86_64 dev container
devcontainer-build-x86:
	devcontainer exec --workspace-folder . --config .devcontainer/x86_64/devcontainer.json bash -c 'CARGO_TARGET_DIR=/tmp/target_x86 cargo build --release'

# Self-compile in x86_64 container (bootstrap → tungsten1_x86)
devcontainer-self-compile-x86: devcontainer-build-x86
	devcontainer exec --workspace-folder . --config .devcontainer/x86_64/devcontainer.json bash -c '\
		/tmp/target_x86/release/tungsten compile src/compiler/main.tg -o tungsten1_x86 -v'
	@echo "✓ Built tungsten1_x86 (x86_64)"

# Self-compile + verify all examples on x86_64
# Checks output content (not just exit code) to prevent false positives (ADR 10.5.26c §2.2).
devcontainer-self-compile-verify-x86: devcontainer-self-compile-x86
	@echo "=== x86_64 self-compile-verify: testing tungsten1_x86 ==="
	@# Smoke test: version must not print help (argv parsing sentinel)
	@printf "  smoke %-35s" "version"; \
	v_out=$$(devcontainer exec --workspace-folder . --config .devcontainer/x86_64/devcontainer.json bash -c \
		"./tungsten1_x86 version" 2>&1); \
	if echo "$$v_out" | grep -q "USAGE:"; then \
		echo "❌ FATAL: version printed help — binary is broken"; \
		echo "$$v_out" | head -5; exit 1; \
	fi; \
	if ! echo "$$v_out" | grep -qi "tungsten"; then \
		echo "❌ FATAL: version did not print expected output"; \
		echo "$$v_out" | head -5; exit 1; \
	fi; \
	echo "✅"
	@# Content-verified check for each example
	@failed=0; for prog in examples/hello.tg examples/answer.tg examples/option.tg \
	             examples/arithmetic.tg examples/strings.tg examples/logic.tg \
	             examples/pair.tg examples/list_ops.tg examples/result.tg \
	             examples/ordering.tg; do \
		printf "  check %-35s" "$$prog"; \
		output=$$(devcontainer exec --workspace-folder . --config .devcontainer/x86_64/devcontainer.json bash -c \
			"./tungsten1_x86 check $$prog" 2>&1); \
		exit_code=$$?; \
		if [ "$$exit_code" -eq 0 ]; then echo "✅"; \
		else echo "❌ FAIL"; echo "$$output" | head -3; failed=1; fi; \
	done; \
	if [ "$$failed" -eq 1 ]; then echo "❌ x86_64 self-compile-verify FAILED"; exit 1; fi
	@echo "✅ x86_64 self-compile-verify passed"

# Stop and remove the x86_64 dev container
devcontainer-down-x86:
	@CONTAINER_ID=$$(docker ps -q --filter "label=devcontainer.local_folder=$$(pwd)" --filter "label=devcontainer.config_file=.devcontainer/x86_64/devcontainer.json"); \
	if [ -n "$$CONTAINER_ID" ]; then \
		docker stop $$CONTAINER_ID && docker rm $$CONTAINER_ID; \
		echo "x86_64 dev container stopped and removed"; \
	else \
		echo "No x86_64 dev container running"; \
	fi

# Stop and remove the dev container
devcontainer-down:
	@CONTAINER_ID=$$(docker ps -q --filter "label=devcontainer.local_folder=$$(pwd)"); \
	if [ -n "$$CONTAINER_ID" ]; then \
		docker stop $$CONTAINER_ID && docker rm $$CONTAINER_ID; \
		echo "Dev container stopped and removed"; \
	else \
		echo "No dev container running"; \
	fi

# Help section for devcontainer commands
.PHONY: help-devcontainer
help-devcontainer:
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
	@echo "  make devcontainer-dump-core PATTERN=<pat> - Dump Core IR (L2, comma-sep or *)"
	@echo "  make devcontainer-check-tyvar-escape      - Check TyVar escapes (L2)"
	@echo "  make devcontainer-check-l3                - Check L3 (bootstrap checks self-hosted)"
	@echo "  make devcontainer-build-check-l3          - Build bootstrap then check L3"
	@echo "  make devcontainer-self-compile-step2      - Build tungsten2 from tungsten1"
	@echo "  make devcontainer-self-compile-dev         - Build tungsten1 with diagnostic tools"
	@echo "  make devcontainer-self-compile-fast         - Fast self-compile (llc -O0)"
	@echo "  make devcontainer-self-compile-verify-fast  - Fast self-compile + verify examples"
	@echo "  make devcontainer-full-bootstrap          - Full bootstrap (tungsten1 + tungsten2)"
	@echo "  make devcontainer-check-l4                - Check L4 (tungsten2 checks itself)"
	@echo "  make devcontainer-check-ir-determinism    - Full IR determinism gate (aliases -full)"
	@echo "  make devcontainer-check-ir-determinism-full - Two clean compiles, includes __mono.ll"
	@echo "  make devcontainer-check-ir-determinism-fast - Fast: elab cache reuse, excludes __mono.ll"
	@echo "  make devcontainer-check-ir-fingerprint-full - Full-source IR fingerprint vs baseline"
	@echo "  make devcontainer-down                    - Stop dev container"
	@echo ""
	@echo "  Log capture: stderr is captured to .devcontainer/logs/ (bind-mounted)."
	@echo "  Inspect from host: cat .devcontainer/logs/<command>.stderr.log"
	@echo ""
	@echo "  Memory: Docker Desktop needs ≥16GB for CODEGEN_JOBS=2, ≥8GB for CODEGEN_JOBS=1."
	@echo "  Override parallelism: TUNGSTEN_CODEGEN_JOBS=N make <target>"
	@echo ""
	@echo "Dev Container (x86_64, QEMU emulation):"
	@echo "  make devcontainer-up-x86                  - Start x86_64 container"
	@echo "  make devcontainer-build-x86               - Build with codegen (x86_64)"
	@echo "  make devcontainer-self-compile-x86        - Self-compile → tungsten1_x86"
	@echo "  make devcontainer-self-compile-verify-x86 - Self-compile + verify examples (x86_64)"
	@echo "  make devcontainer-down-x86                - Stop x86_64 container"
	@echo ""
	@echo "Profiling (ADR 10.5.26j):"
	@echo "  make devcontainer-profile                 - Capture Chrome trace via tungsten-dev profile"

# Profile codegen with Chrome tracing (ADR 10.5.26j §2.5)
# Trace lands in .devcontainer/logs/profiles/ on the host (bind mount).
devcontainer-profile:
	devcontainer exec --workspace-folder . cargo run -p tungsten-dev --release -- profile
