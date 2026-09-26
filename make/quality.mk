# make/quality.mk — Code quality, structural checks, doctor/self-test targets
#
# The public half of the quality surface: lint/fmt, the structural duplication
# checks, the compiler self-test aliases, and the .tg and IR gate families in
# make/quality/ (ADR 31.7.26c). The check-health umbrella and the gates that
# drive tools/ crates live in make/private/quality.mk, which does not ship
# (ADR 25.9.26l D5).

.PHONY: lint fmt check-module-overlap check-ref-resolver-duplication check-walker-discipline self-test self-test-full doctor-self-test doctor-self-test-full check-type-health check-phase-invariants

include make/quality/tg-tests.mk
include make/quality/ir-audits.mk

## Check formatting + clippy (mirrors CI). The publisher's clippy is
## `lint-publisher`, in the private half: the crate does not ship.
lint:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy -p tungsten_core -p tungsten_bootstrap --no-default-features -- -D warnings

## Auto-fix formatting
fmt:
	$(CARGO) fmt --all

## Detect foo.rs + foo/mod.rs coexistence (E0761) — cost 1, host-side.
## A stale pre-split file shipped live in commit dab9144e (found during ADR
## 2.7.26b): the checker existed but nothing ran it routinely.
check-module-overlap:
	$(CARGO) run -q -p tungsten_bootstrap --no-default-features -- doctor check module overlap

## Detect a re-introduced type-reference-resolution twin (ADR 23.7.26a) —
## cost 1, host-side. The Phase-1d and encoding reference resolvers were
## unified into one `walk_type_refs` over a `TypeRefStrategy` (ref_walk.rs), so
## their load-bearing `@`-handling asymmetry (the mechanism of the 22.7.26d
## record-body-freeze bug) lives in exactly one place. The two TyVar leaf
## resolvers — `resolve_tyvar_definition` (@-stripping / Deferred) and
## `resolve_type_ref_tyvar` (bare-name / Encoding) — must therefore be CALLED
## only from `walk_type_refs`; a new caller means someone is re-implementing
## reference resolution (a third twin) instead of routing through the strategy.
## (The broader "all hand-rolled Type walkers → children()/map_children()"
## sweep is ADR 23.7.26b.)
## Allowlist:
##   - types/ref_walk.rs   the sole unified caller (walk_type_refs)
check-ref-resolver-duplication:
	@matches=$$(grep -rn "\.\(resolve_tyvar_definition\|resolve_type_ref_tyvar\)(" \
	    --include="*.rs" bootstrap/src 2>/dev/null \
	    | grep -v "bootstrap/src/elaborate/types/ref_walk.rs"); \
	if [ -n "$$matches" ]; then \
	    echo "✗ type-reference resolver called outside walk_type_refs (ADR 23.7.26a):"; \
	    echo "$$matches"; \
	    echo "  resolve_tyvar_definition / resolve_type_ref_tyvar are the unified"; \
	    echo "  walker's leaf resolvers — route new reference resolution through"; \
	    echo "  Elaborator::walk_type_refs(ty, TypeRefStrategy::_, stack), not a"; \
	    echo "  new hand-rolled twin."; \
	    exit 1; \
	else \
	    echo "✓ Type-reference resolvers called only from walk_type_refs"; \
	fi

## Detect a hand-rolled Type walker re-introduced into a file migrated onto
## the children()/map_children() discipline (ADR 23.7.26b) — cost 1, host-side.
## The tell-tale signature is the uniform binary or-pattern
## `Type::Arrow(..) | Type::Product(..) | Type::Sum(..)`: a migrated walker
## never needs it (the structural default covers those arms), so its
## reappearance in a discipline file means someone re-enumerated the variants
## by hand — the "add a Type variant ⇒ silently-wrong walker" hazard the sweep
## retired. Scope is the MIGRATED files only, by design: the foundational
## primitives (Type::substitute / free_type_vars / is_well_formed,
## compile/validation.rs fold/unfold, codegen analysis) are permanently out of
## the sweep (23.7.26b §3) and are not scanned.
WALKER_DISCIPLINE_FILES := \
    tungsten_core/src/types/tyvar_ops.rs \
    bootstrap/src/fold_analysis.rs \
    bootstrap/src/elaborate/types/encoding_utils.rs \
    bootstrap/src/elaborate/types/ref_walk.rs \
    bootstrap/src/elaborate/types/resolve_refs.rs \
    bootstrap/src/elaborate/resolve_tyvars.rs \
    bootstrap/src/elaborate/phase_checks/tyvar_collectors.rs \
    bootstrap/src/doctor/checks/type_checks/check_constructor_stubs.rs

check-walker-discipline:
	@matches=$$(grep -nE "Type::(Arrow|Product|Sum)\([^)]*\) *\| *Type::(Arrow|Product|Sum)\(" \
	    $(WALKER_DISCIPLINE_FILES) 2>/dev/null); \
	if [ -n "$$matches" ]; then \
	    echo "✗ hand-rolled Type walker re-introduced in a discipline file (ADR 23.7.26b):"; \
	    echo "$$matches"; \
	    echo "  These files' walkers delegate uniform arms to Type::children() /"; \
	    echo "  Type::map_children() — override only the non-uniform arm(s) instead"; \
	    echo "  of re-enumerating the structural variants by hand."; \
	    exit 1; \
	else \
	    echo "✓ Walker-discipline files free of hand-rolled structural arm enumeration"; \
	fi

# Short-form aliases (ADR 16.4.26b §2)
self-test: doctor-self-test
self-test-full: doctor-self-test-full

# Run the compiler self-test suite (requires release build with codegen)
doctor-self-test:
	$(CARGO) run -p tungsten_bootstrap --release -- doctor self-test

# Run the full compiler self-test suite including extended programs
doctor-self-test-full:
	$(CARGO) run -p tungsten_bootstrap --release -- doctor self-test --full

## Run type encoding health checks (encoding depth + type sizes)
check-type-health:
	$(CARGO) run -p tungsten_bootstrap --release -- doctor check encoding-depth examples/list.tg
	$(CARGO) run -p tungsten_bootstrap --release -- doctor check type-sizes examples/list.tg

## Run phase invariant checks on example programs
check-phase-invariants:
	$(CARGO) run -p tungsten_bootstrap --release -- doctor check type integrity phase-invariants examples/list.tg


HELP_SECTIONS += quality

# Help section for quality
.PHONY: help-quality
help-quality:
	@echo ""
	@echo "Quality & Health:"
	@echo "  make check-module-overlap - Detect foo.rs + foo/mod.rs coexistence (E0761)"
	@echo "  make check-ref-resolver-duplication - Detect a re-introduced type-reference-resolution twin (ADR 23.7.26a)"
	@echo "  make check-walker-discipline - Detect hand-rolled Type walkers in children()/map_children() files (ADR 23.7.26b)"
	@echo "  make check-test-runner  - Verify the tungsten test runner consults the failure flag (ADR 29.6.26f)"
	@echo "  make check-ast-compare-nonvacuous - Prove the AST comparison suite actually asserts (ADR 29.6.26f)"
	@echo "  make check-comparator-repros - Comparator regression fixtures must return 0 (ADR 1.8.26b)"
	@echo "  make check-mustfail-fixtures - Every mustfail_*.tg is red, for its own declared reason (ADR 7.8.26a)"
	@echo "  make lint               - Check formatting + clippy (mirrors CI)"
	@echo "  make fmt                - Auto-fix formatting"
	@echo "  make check-type-health  - Run type encoding health checks"
	@echo "  make check-phase-invariants - Run elaboration phase invariant checks"
	@echo "  make self-test          - Run compiler self-test suite"
	@echo "  make self-test-full     - Run extended self-test suite"
	@echo "  make tg-test            - Run all .tg unit tests (self-hosted compiler)"
	@echo "  make tg-test-module MODULE=<path> - Run .tg tests for a single module"
	@echo "  make tg-test-codegen    - Run codegen emitter .tg tests (ADR 13.5.26j)"
	@echo "  make tg-test-dead-arm  - Run dead-arm let-else regression (ADR 3.7.26a)"
	@echo "  make check-ir-determinism-canary - Fast IR determinism check (host-side, ~3s)"
	@echo "  make check-cache-poisoning-canary - Cold-vs-warm cache parity (host-side, ~2s)"
	@echo "  make check-extern-map-ambiguity  - Colliding-name extern-map audit of the self-hosted compiler (ADR 12.7.26b)"
	@echo "  make check-ir-audits             - All six dir-scanning doctor check ir audits over one freshly emitted compiler corpus (ADR 28.7.26e)"
	@echo "  make check-indirect-buffers      - Class-P indirect-buffer audit alone — the noalias oracle fast path (ADR 17.7.26e)"
	@echo "  make check-arena-mode            - diff exec under TUNGSTEN_ARENA=bump:1 + no @malloc call outside an alloc.malloc block (ADR 14.9.26b, 18.9.26c)"
	@echo "  make check-arena-branch-cost     - In-container three-variant same-IR A/B of the allocation-site mode branch (ADR 18.9.26c)"
	@echo "  make check-int-parity            - diff exec over the Int parity battery + the overflow-trap fixture (ADR 14.9.26c)"
	@echo "  make check-ir-fingerprint-canary - Compare canary IR against baseline (ADR 18.5.26d)"
	@echo "  make update-ir-fingerprint       - Update canary IR fingerprint baseline"
	@echo "  make golden             - Run golden snapshot tests"
	@echo "  make golden-update      - Update golden snapshot expected files"
