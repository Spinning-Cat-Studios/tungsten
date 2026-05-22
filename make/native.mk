# make/native.mk — Native self-compile targets (Mac LLVM 18)
#
# Self-compilation pipeline: bootstrap → tungsten1 → tungsten2
# Requires: brew install llvm@18

.PHONY: self-compile self-compile-fast build-bootstrap ensure-lib-symlink self-compile-step2 full-bootstrap self-compile-ir self-compile-llc self-compile-llc-fast self-compile-link self-compile-verify self-compile-verify-quick self-compile-dev

# Full self-compile: per-module IR → object files → binary (native Mac)
self-compile:
	@echo "=== Stage 0/3: Building bootstrap compiler and FFI library ==="
	cargo build --release -p tungsten_bootstrap -p tungsten_core -p tungsten_codegen
	@echo ""
	@echo "=== Stage 1/3: Generating LLVM IR (per-module) ==="
	mkdir -p tungsten1_ll
	./target/release/tungsten compile src/compiler/main.tg --emit-llvm -o tungsten1_ll/ -v
	@echo ""
	@echo "=== Stage 2/3: Compiling IR to object files (llc) ==="
	@echo "Using $(LLC)"
	@for f in tungsten1_ll/*.ll; do echo "  llc $$f"; $(LLC) -filetype=obj "$$f" -o "$${f%.ll}.o" -O2 --stats; done
	@echo ""
	@echo "=== Stage 3/3: Linking ==="
	cc tungsten1_ll/*.o -o tungsten1 target/release/libtungsten_core.a -lSystem -lc -lm
	rm -rf tungsten1_ll
	@echo ""
	@echo "✓ Built tungsten1"

# Fast self-compile with -O0 (for testing, ~10-20x faster llc)
self-compile-fast:
	@echo "=== Stage 0/3: Building bootstrap compiler and FFI library ==="
	cargo build --release -p tungsten_bootstrap -p tungsten_core -p tungsten_codegen
	@echo ""
	@echo "=== Stage 1/3: Generating LLVM IR (per-module) ==="
	mkdir -p tungsten1_ll
	./target/release/tungsten compile src/compiler/main.tg --emit-llvm -o tungsten1_ll/ -v
	@echo ""
	@echo "=== Stage 2/3: Compiling IR to object files (llc -O0) ==="
	@echo "Using $(LLC) with -O0 for speed"
	@for f in tungsten1_ll/*.ll; do echo "  llc $$f"; $(LLC) -filetype=obj "$$f" -o "$${f%.ll}.o" -O0; done
	@echo ""
	@echo "=== Stage 3/3: Linking ==="
	cc tungsten1_ll/*.o -o tungsten1 target/release/libtungsten_core.a -lSystem -lc -lm
	rm -rf tungsten1_ll
	@echo ""
	@echo "✓ Built tungsten1 (unoptimized)"

build-bootstrap:
	cargo build --release -p tungsten_bootstrap -p tungsten_core -p tungsten_codegen

# No longer needed — static linking eliminates runtime library dependency
ensure-lib-symlink:
	@true

# Step 2: tungsten1 compiles itself to tungsten2 (native Mac)
self-compile-step2:
	@if [ ! -f tungsten1 ]; then \
		echo "Error: tungsten1 not found. Run 'make self-compile' first."; \
		exit 1; \
	fi
	./tungsten1 compile src/compiler/main.tg -o tungsten2 -v
	@echo "✓ Built tungsten2 (self-hosted compiler compiled by tungsten1)"

# Full bootstrap: Step 1 + Step 2 (native Mac)
full-bootstrap: self-compile self-compile-step2
	@echo ""
	@echo "=== Bootstrap Complete ==="
	@ls -lh tungsten1 tungsten2
	@echo ""
	@echo "To verify L4: ./tungsten2 check src/compiler/main.tg"

# Generate IR only (native) — writes per-module .ll files to tungsten1_ll/
self-compile-ir:
	mkdir -p tungsten1_ll
	cargo run -p tungsten_bootstrap --release -- compile src/compiler/main.tg --emit-llvm -o tungsten1_ll/ -v

# Compile per-module IR to object files (requires tungsten1_ll/ from self-compile-ir)
self-compile-llc:
	@echo "Using $(LLC)"
	@for f in tungsten1_ll/*.ll; do echo "  llc $$f"; $(LLC) -filetype=obj "$$f" -o "$${f%.ll}.o" -O2 -time-passes --stats; done

# Compile per-module IR to object files with -O0 (fast, requires tungsten1_ll/)
self-compile-llc-fast:
	@echo "Using $(LLC) with -O0"
	@for f in tungsten1_ll/*.ll; do echo "  llc $$f"; $(LLC) -filetype=obj "$$f" -o "$${f%.ll}.o" -O0; done

# Link object files to binary (requires tungsten1_ll/*.o from self-compile-llc)
self-compile-link:
	cc tungsten1_ll/*.o -o tungsten1 target/release/libtungsten_core.a -lSystem -lc -lm
	@echo "✓ Linked tungsten1"

# Single-command confidence check: build tungsten1 + verify it type-checks all test programs
self-compile-verify: self-compile
	@echo "=== Self-compile-verify: testing tungsten1 ==="
	@failed=0; for prog in examples/hello.tg examples/answer.tg examples/option.tg \
	             examples/arithmetic.tg examples/strings.tg examples/logic.tg \
	             examples/pair.tg examples/list_ops.tg examples/result.tg \
	             examples/ordering.tg; do \
		printf "  check %-35s" "$$prog"; \
		if ./tungsten1 check "$$prog" >/dev/null 2>&1; then echo "✅"; \
		else echo "❌ FAIL"; failed=1; fi; \
	done; \
	if [ "$$failed" -eq 1 ]; then echo "❌ Self-compile-verify FAILED"; exit 1; fi
	@echo "✅ Self-compile-verify: tungsten1 passed all 10 checks"

# Quick variant (default tier — 3 programs, for iteration speed)
self-compile-verify-quick: self-compile
	@echo "=== Self-compile-verify-quick: testing tungsten1 ==="
	@failed=0; for prog in examples/hello.tg examples/answer.tg examples/option.tg; do \
		printf "  check %-35s" "$$prog"; \
		if ./tungsten1 check "$$prog" >/dev/null 2>&1; then echo "✅"; \
		else echo "❌ FAIL"; failed=1; fi; \
	done; \
	if [ "$$failed" -eq 1 ]; then echo "❌ Self-compile-verify-quick FAILED"; exit 1; fi
	@echo "✅ Self-compile-verify-quick: tungsten1 passed smoke test"

# Developer build: self-compile with diagnostic tools enabled (ADR 18.4.26f §4.5)
self-compile-dev:
	@echo "=== Building tungsten1 (developer mode — diagnostic tools enabled) ==="
	@# Step 1: Swap in developer diagnostics
	@cp src/compiler/driver/ffi/diagnostics.tg src/compiler/driver/ffi/diagnostics.tg.prod
	@cp src/compiler/driver/ffi/diagnostics_dev.tg src/compiler/driver/ffi/diagnostics.tg
	@# Step 2: Build (same as self-compile)
	@$(MAKE) self-compile || { \
		cp src/compiler/driver/ffi/diagnostics.tg.prod src/compiler/driver/ffi/diagnostics.tg; \
		rm -f src/compiler/driver/ffi/diagnostics.tg.prod; \
		exit 1; \
	}
	@# Step 3: Restore production stubs
	@cp src/compiler/driver/ffi/diagnostics.tg.prod src/compiler/driver/ffi/diagnostics.tg
	@rm -f src/compiler/driver/ffi/diagnostics.tg.prod
	@echo "✓ Built tungsten1 (developer mode)"

# Help section for native compilation
.PHONY: help-native
help-native:
	@echo ""
	@echo "Native Mac Self-Compile (brew install llvm@18):"
	@echo "  make self-compile             - Build tungsten1 (IR→object→link)"
	@echo "  make self-compile-fast        - Build tungsten1 with -O0 (faster)"
	@echo "  make self-compile-dev         - Build tungsten1 with diagnostic tools"
	@echo "  make self-compile-ir          - Generate tungsten1.ll only"
	@echo "  make self-compile-llc         - Compile tungsten1.ll → .o (with stats)"
	@echo "  make self-compile-llc-fast    - Compile tungsten1.ll → .o (-O0)"
	@echo "  make self-compile-link        - Link tungsten1.o → tungsten1"
	@echo "  make self-compile-step2       - Step 2: tungsten1 → tungsten2"
	@echo "  make full-bootstrap           - Full bootstrap (tungsten1 + tungsten2)"
