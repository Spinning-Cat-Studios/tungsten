# make/native.mk — Native self-compile targets (Mac LLVM 18)
#
# Self-compilation pipeline: bootstrap → tungsten1 → tungsten2
# Requires: brew install llvm@18

.PHONY: self-compile self-compile-fast build-bootstrap ensure-lib-symlink self-compile-step2 full-bootstrap self-compile-ir self-compile-llc self-compile-llc-fast self-compile-link self-compile-verify self-compile-verify-quick self-compile-dev selfcompiled-stored-generics-check selfcompiled-rss-gate selfcompiled-rss-gate-arena selfhost-tools-fresh

# Full self-compile: build tungsten1 via the in-process codegen pipeline.
#
# HISTORY: this used to hand-roll --emit-llvm → llc → cc. Per-function
# emission (ADR 9.5.26b) made that flow doubly broken here: the `.ll` files
# land in a mirror DIRECTORY TREE (a flat `*.ll` glob linked 1 of ~2000
# units), and on case-insensitive APFS same-named-modulo-case units
# (`char_A.ll` / `char_a.ll`) silently overwrite each other in the tree, so
# even a `find`-based llc pass links with missing symbols. The real compile
# pipeline avoids both: in-process codegen emits index-prefixed object files
# (`0_char_A.o`, `1_char_a.o`) and links them itself.
self-compile:
	@echo "=== Stage 1/2: Building bootstrap compiler and FFI library ==="
	$(CARGO) build --release -p tungsten_bootstrap -p tungsten_core -p tungsten_codegen
	@echo ""
	@echo "=== Stage 2/2: Compiling the self-hosted compiler (in-process codegen) ==="
	./target/release/tungsten compile src/compiler/main.tg -o tungsten1 -v
	@echo ""
	@echo "✓ Built tungsten1"

# Alias retained for muscle memory: the in-process pipeline has no llc -O0/-O2
# split, so "fast" and full are the same flow now.
self-compile-fast: self-compile

build-bootstrap:
	$(CARGO) build --release -p tungsten_bootstrap -p tungsten_core -p tungsten_codegen

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
	$(CARGO) run -p tungsten_bootstrap --release -- compile src/compiler/main.tg --emit-llvm -o tungsten1_ll/ -v

# Compile per-module IR to object files (requires tungsten1_ll/ from
# self-compile-ir). Tree-aware: --emit-llvm writes a mirror directory tree
# (ADR 9.5.26b), so walk it with find — a flat glob sees ~1 of ~2000 units.
# CAVEAT (IR analysis only, not a link source of truth): on case-insensitive
# APFS, same-named-modulo-case units (char_A.ll / char_a.ll) overwrite each
# other in the tree, so a binary linked from it is missing those few symbols.
# Build runnable binaries with `make self-compile` (in-process pipeline,
# index-prefixed objects).
self-compile-llc:
	@echo "Using $(LLC)"
	@find tungsten1_ll -name '*.ll' -print0 | xargs -0 -I{} sh -c 'echo "  llc $$1"; $(LLC) -filetype=obj "$$1" -o "$${1%.ll}.o" -O2 -time-passes --stats' _ {}

# Compile per-module IR to object files with -O0 (fast, requires tungsten1_ll/)
self-compile-llc-fast:
	@echo "Using $(LLC) with -O0"
	@find tungsten1_ll -name '*.ll' -print0 | xargs -0 -P4 -I{} sh -c '$(LLC) -filetype=obj "$$1" -o "$${1%.ll}.o" -O0' _ {}

# Link object files to binary (requires tungsten1_ll/ objects from
# self-compile-llc). See the APFS caveat above — prefer `make self-compile`.
self-compile-link:
	cc $$(find tungsten1_ll -name '*.o') -o tungsten1 target/release/libtungsten_core.a -lSystem -lc -lm
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

# Stored-generic regression battery under the self-compiled compiler (ADR
# 21.7.26k D3): the bootstrap reproducer matrix (ADR 21.7.26e §2.1) mirrored as
# fixtures that tungsten1 itself elaborates — `.tg` tests under `tungsten test`
# run on the already-fixed bootstrap and would guard nothing. Assumes
# ./tungsten1 exists (run in-container after a self-compile). The non-regular
# backstop fixture must FAIL with a rendered error; everything else must pass.
selfcompiled-stored-generics-check:
	@echo "=== Stored-generic battery: tungsten1 checking the 21.7.26e matrix ==="
	@test -x ./tungsten1 || { echo "❌ ./tungsten1 missing — run a self-compile first"; exit 1; }
	@failed=0; for prog in tests/selfcompiled_stored_generics/field_list.tg \
	             tests/selfcompiled_stored_generics/field_tree.tg \
	             tests/selfcompiled_stored_generics/payload_list.tg \
	             tests/selfcompiled_stored_generics/payload_tree.tg \
	             tests/selfcompiled_stored_generics/poly3.tg \
	             tests/golden/check/cross_module_stored_generic_field/main.tg \
	             tests/golden/check/cross_module_stored_generic_payload/main.tg; do \
		printf "  check %-60s" "$$prog"; \
		if ./tungsten1 check "$$prog" >/dev/null 2>&1; then echo "✅"; \
		else echo "❌ FAIL"; failed=1; fi; \
	done; \
	printf "  reject %-59s" "tests/selfcompiled_stored_generics/nonregular_backstop.tg"; \
	if ./tungsten1 check tests/selfcompiled_stored_generics/nonregular_backstop.tg >/dev/null 2>&1; then \
		echo "❌ FAIL (accepted a non-regular generic)"; failed=1; \
	else echo "✅ (rendered error)"; fi; \
	if [ "$$failed" -eq 1 ]; then echo "❌ Stored-generic battery FAILED"; exit 1; fi
	@echo "✅ Stored-generic battery: all cells green"

# Self-compiled self-check RSS-ceiling regression guard (ADR 23.7.26d AC4). The
# self-check runs on the never-freeing (leak-on-drop) runtime, so peak RSS is
# the integral of allocation; a hot-path retained structure can silently ramp it
# back toward the ~31 GiB container VM (as the O(M²) re-injection did: ~9.5 →
# ~25.8 GiB before this ADR's O(M) fix dropped it to ~2.1 GiB). This gate re-uses
# the selfcompiled-profile 1 Hz RSS sampler to run `tungsten1 check main.tg` under a cap and
# FAIL if the check is killed at the ceiling or exits non-zero — tripping well
# before the VM line. Assumes ./tungsten1 exists; `tungsten-dev` is rebuilt by
# `selfhost-tools-fresh` (ADR 24.9.26a) — the target dir is resolved rather than hard-coded because in the
# devcontainer it is a named volume (/build/target, ADR 24.7.26b) while the
# bind-mounted ./target holds HOST artefacts: hard-coding it ran a stale July
# binary that predates this subcommand, so the gate failed with "check did not
# exit 0" for a reason unrelated to the code under test (found by ADR 19.8.26d)
# (run in-container after a self-compile). Ceiling default 8 GiB (~4× the
# post-fix ~2.1 GiB peak, room for compiler growth); override TUNGSTEN_SELFCOMPILED_RSS_CEILING_GB.
TUNGSTEN_SELFCOMPILED_RSS_CEILING_GB ?= 8

# A gate must prove its own tooling is fresh (ADR 24.9.26a). Every self-host
# gate below that drives the bootstrap `tungsten` or `tungsten-dev` depends on
# this rather than a `test -x` of whatever binary is on disk: it builds
# `tungsten-dev`, then runs the canonical bootstrap build `tungsten-dev
# self-compile` runs first, so Cargo — which knows every input — decides
# whether either is stale. A failed build stops the gate. There is no opt-out.
selfhost-tools-fresh:
	$(CARGO) build --release -p tungsten-dev
	$(DC_TARGET)/release/tungsten-dev ensure-bootstrap

selfcompiled-rss-gate: selfhost-tools-fresh
	@echo "=== Self-compiled RSS gate: tungsten1 check main.tg under $(TUNGSTEN_SELFCOMPILED_RSS_CEILING_GB) GiB ==="
	@test -x ./tungsten1 || { echo "❌ ./tungsten1 missing — run a self-compile first"; exit 1; }
	@out=$$($${CARGO_TARGET_DIR:-target}/release/tungsten-dev selfcompiled-profile --skip-build \
	          --max-rss-gb $(TUNGSTEN_SELFCOMPILED_RSS_CEILING_GB) --top 1 2>&1); \
	echo "$$out" | grep -E "terminal:|outcome:"; \
	if echo "$$out" | grep -q "KILLED at"; then \
		echo "❌ Self-compiled RSS gate: check crossed the $(TUNGSTEN_SELFCOMPILED_RSS_CEILING_GB) GiB ceiling — a hot-path retained structure regressed (ADR 23.7.26d)"; exit 1; \
	elif echo "$$out" | grep -q "outcome: check exited 0"; then \
		echo "✅ Self-compiled RSS gate: check completed under the ceiling"; \
	else \
		echo "❌ Self-compiled RSS gate: check did not exit 0"; exit 1; \
	fi

# The same gate with the bump arena selected (ADR 14.9.26b AC 5). TUNGSTEN_ARENA
# is exported into the RUN only: tungsten-dev and the bootstrap never call
# __tungsten_arena_init, so it reaches nothing but the tungsten1 child, whose
# prologue reads it. Same ceiling, same sampler, same target-dir resolution;
# override TUNGSTEN_ARENA_MODE for a chunk size (e.g. bump:16).
TUNGSTEN_ARENA_MODE ?= bump
selfcompiled-rss-gate-arena: selfhost-tools-fresh
	@echo "=== Self-compiled RSS gate (TUNGSTEN_ARENA=$(TUNGSTEN_ARENA_MODE)): tungsten1 check main.tg under $(TUNGSTEN_SELFCOMPILED_RSS_CEILING_GB) GiB ==="
	@test -x ./tungsten1 || { echo "❌ ./tungsten1 missing — run a self-compile first"; exit 1; }
	@out=$$(TUNGSTEN_ARENA=$(TUNGSTEN_ARENA_MODE) $${CARGO_TARGET_DIR:-target}/release/tungsten-dev selfcompiled-profile --skip-build \
	          --max-rss-gb $(TUNGSTEN_SELFCOMPILED_RSS_CEILING_GB) --top 1 2>&1); \
	echo "$$out" | grep -E "terminal:|outcome:"; \
	if echo "$$out" | grep -q "KILLED at"; then \
		echo "❌ Self-compiled RSS gate (arena): check crossed the $(TUNGSTEN_SELFCOMPILED_RSS_CEILING_GB) GiB ceiling under TUNGSTEN_ARENA=$(TUNGSTEN_ARENA_MODE)"; exit 1; \
	elif echo "$$out" | grep -q "outcome: check exited 0"; then \
		echo "✅ Self-compiled RSS gate (arena): check completed under the ceiling"; \
	else \
		echo "❌ Self-compiled RSS gate (arena): check did not exit 0"; exit 1; \
	fi

# Developer build: self-compile with diagnostic tools enabled (ADR 18.4.26f §4.5)
self-compile-dev:
	@echo "=== Building tungsten1 (developer mode — diagnostic tools enabled) ==="
	@# Step 1: Swap in developer diagnostics
	@cp src/compiler/driver/ffi/diagnostics/mod.tg src/compiler/driver/ffi/diagnostics/mod.tg.prod
	@cp src/compiler/driver/ffi/diagnostics/dev.tg src/compiler/driver/ffi/diagnostics/mod.tg
	@# Step 2: Build (same as self-compile)
	@$(MAKE) self-compile || { \
		cp src/compiler/driver/ffi/diagnostics/mod.tg.prod src/compiler/driver/ffi/diagnostics/mod.tg; \
		rm -f src/compiler/driver/ffi/diagnostics/mod.tg.prod; \
		exit 1; \
	}
	@# Step 3: Restore production stubs
	@cp src/compiler/driver/ffi/diagnostics/mod.tg.prod src/compiler/driver/ffi/diagnostics/mod.tg
	@rm -f src/compiler/driver/ffi/diagnostics/mod.tg.prod
	@echo "✓ Built tungsten1 (developer mode)"

# Help section for native compilation
.PHONY: help-native
help-native:
	@echo ""
	@echo "Native Mac Self-Compile (brew install llvm@18):"
	@echo "  make self-compile             - Build tungsten1 (in-process codegen + link)"
	@echo "  make self-compile-fast        - Alias of self-compile (kept for muscle memory)"
	@echo "  make self-compile-dev         - Build tungsten1 with diagnostic tools"
	@echo "  make self-compile-ir          - Generate tungsten1.ll only"
	@echo "  make self-compile-llc         - Compile tungsten1.ll → .o (with stats)"
	@echo "  make self-compile-llc-fast    - Compile tungsten1.ll → .o (-O0)"
	@echo "  make self-compile-link        - Link tungsten1.o → tungsten1"
	@echo "  make self-compile-step2       - Step 2: tungsten1 → tungsten2"
	@echo "  make full-bootstrap           - Full bootstrap (tungsten1 + tungsten2)"
	@echo "  make selfcompiled-stored-generics-check - Stored-generic battery under tungsten1 (ADR 21.7.26k)"
	@echo "  make selfcompiled-rss-gate              - Self-compiled RSS-ceiling regression guard (ADR 23.7.26d)"
	@echo "  make selfcompiled-rss-gate-arena        - The same gate under TUNGSTEN_ARENA=bump (ADR 14.9.26b)"

HELP_SECTIONS += native
