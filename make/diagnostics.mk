# make/diagnostics.mk — Diagnostic tool targets
#
# Commands for running diagnostic tools on the self-hosted compiler.
# These wrap the bootstrap's --dump-core and --check-tyvar-escape flags.

.PHONY: dump-core check-tyvar-escape check-tco-gate check-tool-reachability

# Dump Core IR for matching definitions (native Mac, release build)
dump-core:
ifndef PATTERN
	@echo "Usage: make dump-core PATTERN=<pattern>"
	@echo "Examples:"
	@echo "  make dump-core PATTERN=main"
	@echo "  make dump-core PATTERN='*'"
else
	$(CARGO) run -p tungsten_bootstrap --release -- check src/compiler/main.tg --dump-core "$(PATTERN)" --max-errors=0
endif

# Check TyVar escapes in the self-hosted compiler (native Mac, release build)
check-tyvar-escape:
	$(CARGO) run -p tungsten_bootstrap --release -- check src/compiler/main.tg --check-tyvar-escape --max-errors=0

## The deterministic musttail TCO gate (ADR 1.7.26e R7/R8), wired into a make
## target and CI by ADR 5.8.26a. Until then it had neither: fully implemented,
## documented by `info pipeline` as "the deterministic CI form", and invoked by
## nothing — which is why 17.7.26d could park mutual-tail Class-P `musttail` as
## "self-guarding: the R8 gate cannot let this land silently". A gate nobody runs
## looks exactly like a gate that passes (the ADR 28.7.26e class, second
## instance).
##
## Lives here rather than in quality.mk deliberately: that file's `mk-size`
## allowlist justification records "the next addition should land in a split
## file, not a third bump", and this is a NEW target. `check-tyvar-escape` beside
## it is the same shape — a check run over the self-hosted compiler.
##
## Fails on any un-allowlisted HIGH-risk SELF-recursive SKIP, and on any
## tools/tco-skip-allowlist.toml entry that participates in recursion. Read the
## ✓ precisely: it does not cover MUTUAL recursion, which needs a call-graph join
## codegen does not have — those edges are recorded as SKIP_NON_SELF and counted,
## not judged (bootstrap/src/compile/tco/gate.rs). Cost 4: elaborates + runs real
## codegen sequentially, output discarded (~2:11 host-side).
check-tco-gate:
	@echo "=== musttail TCO gate over the self-hosted compiler (ADR 1.7.26e R8) ==="
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- \
	  doctor check codegen tco-coverage $(COMPILER_MAIN) --gate

HELP_SECTIONS += diagnostics

# Help section for diagnostics
.PHONY: help-diagnostics
help-diagnostics:
	@echo ""
	@echo "Diagnostics:"
	@echo "  make dump-core PATTERN=<pat>   - Dump Core IR for matching defs"
	@echo "  make check-tyvar-escape        - Check TyVar escapes in compiler"
	@echo "  make check-tco-gate            - Deterministic musttail TCO gate (ADRs 1.7.26e R8, 5.8.26a)"

## Do the failure modes' companion diagnostics still report? (ADRs 12.8.26a,
## 13.8.26c) — cost 3.
##
## A gate that aborts elaboration deletes the report aimed at the files it
## rejects, and nothing else notices: the subcommand still exists, still
## resolves, still has the right cost tier, and its --help still describes the
## behaviour it had before the flip. Happened twice — 7.8.26e (positivity) and
## 11.8.26b (termination), the second in an ADR that had read the warning.
##
## "Failure mode", not "hard gate": 13.8.26c added the E0016 row, an ordinary
## elaboration error that aborts just as thoroughly, and widened the table's
## charter to match rather than excluding the pairing that needed it most.
##
## Lives here rather than in quality.mk deliberately: quality.mk's `mk-size`
## allowlist entry says a NEW target must land in a split file rather than take
## another bump, and this is the same shape as `check-tyvar-escape` /
## `check-tco-gate` above — a check run by the compiler over the compiler.
check-tool-reachability:
	$(CARGO) run -q -p tungsten_bootstrap --no-default-features -- doctor tool-reachability
