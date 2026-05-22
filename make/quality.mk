# make/quality.mk — Code quality, complexity checks, doctor/self-test targets
#
# Structural quality audits, complexity thresholds, and compiler health checks.

.PHONY: check-complexity check-repo-audit lint fmt check-health self-test self-test-full doctor-self-test doctor-self-test-full check-type-health check-phase-invariants tg-test tg-test-module tg-test-codegen tg-test-closures tg-test-cir tg-test-if-let tg-test-try-block tg-test-type-alias tg-test-string-concat tg-test-pattern check-ir-determinism-canary check-ir-fingerprint-canary update-ir-fingerprint

## Check file/directory complexity
#
# Regarding cyclomatic complexity:
# ---------------------------------
# In Thomas McCabe's 1976 paper, a CC threshold of 10 was posited as the "gold standard".
# However, we use a CC threshold of 15. This is a pragmatic choice for rust codebases.
#
# In particular, the following aspects of idiomatic Rust can contribute to higher CC without necessarily indicating poor code quality:
# - Pattern matching: Rust's powerful pattern matching can lead to functions with many match arms, which can increase CC.
# - Error handling: Rust's emphasis on robust error handling often results in functions with multiple error
#   handling paths, which can also increase CC.
# - Match arms: Functions that handle complex data structures may have many match arms to cover different cases, contributing to higher CC.
check-complexity:
	cargo run -p code-health --release

## Check formatting + clippy (mirrors CI)
lint:
	cargo fmt --all -- --check
	cargo clippy -p tungsten_core -p tungsten_bootstrap --no-default-features -- -D warnings
	cargo clippy -p tungsten_snapshot 2>&1

## Auto-fix formatting
fmt:
	cargo fmt --all

## Umbrella: complexity + lint
check-health: check-complexity lint

## Run structural debt and architectural overfit audit (report-only)
check-repo-audit:
	cargo run -p code-health --release -- --check repo-audit

# Short-form aliases (ADR 16.4.26b §2)
self-test: doctor-self-test
self-test-full: doctor-self-test-full

# Run the compiler self-test suite (requires release build with codegen)
doctor-self-test:
	cargo run -p tungsten_bootstrap --release -- doctor self-test

# Run the full compiler self-test suite including extended programs
doctor-self-test-full:
	cargo run -p tungsten_bootstrap --release -- doctor self-test --full

## Run type encoding health checks (encoding depth + type sizes)
check-type-health:
	cargo run -p tungsten_bootstrap --release -- doctor check-encoding-depth examples/list.tg
	cargo run -p tungsten_bootstrap --release -- doctor check-type-sizes examples/list.tg

## Run phase invariant checks on example programs
check-phase-invariants:
	cargo run -p tungsten_bootstrap --release -- doctor check-phase-invariants examples/list.tg

## Run all .tg unit tests in the self-hosted compiler (ADR 12.5.26b)
tg-test:
	cargo run -p tungsten_bootstrap --no-default-features -- test src/compiler/main.tg

## Run .tg unit tests scoped to a single module (ADR 12.5.26b)
## Usage: make tg-test-module MODULE=src/compiler/elab/env/mod.tg
tg-test-module:
ifndef MODULE
	$(error MODULE is required, e.g. make tg-test-module MODULE=src/compiler/elab/env/mod.tg)
endif
	cargo run -p tungsten_bootstrap --no-default-features -- test src/compiler/main.tg --module $(MODULE)

## Run .tg codegen emitter tests (ADR 13.5.26j) — cost 5, needs LLVM
tg-test-codegen:
	cargo run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_codegen.tg

## Run .tg closure emitter tests (ADR 13.5.26m) — cost 5, needs LLVM
tg-test-closures:
	cargo run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_codegen_closures.tg

## Run .tg CIR capture list tests (ADR 13.5.26j) — cost 3 (--check-only) or cost 5
tg-test-cir:
	cargo run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_cir_captures.tg

## Run if-let .tg tests (ADR 14.5.26e) — cost 3 (expect_type) + cost 5 (assert_eq_nat)
tg-test-if-let:
	cargo run -p tungsten_bootstrap --no-default-features -- test tests/if_let.tg

## Run try-block .tg tests (ADR 15.5.26d) — cost 3 (expect_type/expect_error)
tg-test-try-block:
	cargo run -p tungsten_bootstrap --no-default-features -- test tests/try_block.tg --check-only

## Run type-alias .tg tests (ADR 15.5.26g) — cost 3 (expect_type/expect_error)
tg-test-type-alias:
	cargo run -p tungsten_bootstrap --no-default-features -- test tests/type_alias.tg --check-only

## Run string concat .tg tests (ADR 18.5.26f) — cost 3 (--check-only) or cost 5
tg-test-string-concat:
	cargo run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_string_concat.tg --check-only

## Run nested pattern .tg tests (ADR 20.5.26a) — cost 3 (expect_type)
tg-test-pattern:
	cargo run -p tungsten_bootstrap --no-default-features -- test tests/pattern_nested_tuple.tg --check-only

## Run list ops .tg tests (ADR 20.5.26e) — cost 3 (--check-only)
tg-test-list-ops:
	cargo run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_list_ops.tg --check-only

## Run golden snapshot tests (all categories)
golden:
	cargo run --release -p golden

## Update golden snapshot expected files
golden-update:
	cargo run --release -p golden -- --update

## Fast IR determinism canary (host-side, ~3s) — ADR 18.5.26b
## Compiles a small .tg file twice with --emit-llvm and diffs the output.
check-ir-determinism-canary:
	@bash scripts/check-ir-determinism-canary.sh

## Compare canary IR output against stored fingerprint baseline (ADR 18.5.26d)
check-ir-fingerprint-canary:
	@bash scripts/check-ir-fingerprint.sh

## Update canary IR fingerprint baseline (ADR 18.5.26d)
update-ir-fingerprint:
	@bash scripts/check-ir-fingerprint.sh --update

# Help section for quality
.PHONY: help-quality
help-quality:
	@echo ""
	@echo "Quality & Health:"
	@echo "  make check-health       - Run all quality checks (complexity + lint)"
	@echo "  make check-complexity   - Run file/dir/function complexity checks"
	@echo "  make lint               - Check formatting + clippy (mirrors CI)"
	@echo "  make fmt                - Auto-fix formatting"
	@echo "  make check-repo-audit   - Run structural debt audit"
	@echo "  make check-type-health  - Run type encoding health checks"
	@echo "  make check-phase-invariants - Run elaboration phase invariant checks"
	@echo "  make self-test          - Run compiler self-test suite"
	@echo "  make self-test-full     - Run extended self-test suite"
	@echo "  make tg-test            - Run all .tg unit tests (self-hosted compiler)"
	@echo "  make tg-test-module MODULE=<path> - Run .tg tests for a single module"
	@echo "  make tg-test-codegen    - Run codegen emitter .tg tests (ADR 13.5.26j)"
	@echo "  make tg-test-closures   - Run closure emitter .tg tests (ADR 13.5.26m)"
	@echo "  make tg-test-cir        - Run CIR capture list .tg tests (ADR 13.5.26j)"
	@echo "  make tg-test-if-let     - Run if-let .tg tests (ADR 14.5.26e)"
	@echo "  make tg-test-try-block  - Run try-block .tg tests (ADR 15.5.26d)"
	@echo "  make tg-test-type-alias - Run type-alias .tg tests (ADR 15.5.26g)"
	@echo "  make tg-test-string-concat - Run string concat .tg tests (ADR 18.5.26f)"
	@echo "  make tg-test-pattern   - Run nested pattern .tg tests (ADR 20.5.26a)"
	@echo "  make tg-test-list-ops  - Run list ops .tg tests (ADR 20.5.26e)"
	@echo "  make check-ir-determinism-canary - Fast IR determinism check (host-side, ~3s)"
	@echo "  make check-ir-fingerprint-canary - Compare canary IR against baseline (ADR 18.5.26d)"
	@echo "  make update-ir-fingerprint       - Update canary IR fingerprint baseline"
	@echo "  make golden             - Run golden snapshot tests"
	@echo "  make golden-update      - Update golden snapshot expected files"
