# Per-file `.tg` suite targets — one convenience target per test entry file.
#
# Extracted from make/quality.mk by ADR 6.8.26c, which needed to ADD two of
# them (`tg-test-compare-result`, `tg-test-string-utils`) and found quality.mk's
# `mk-size` allowlist entry saying, in as many words, that the next NEW target
# should land in a split file rather than a fourth ceiling bump. This is that
# file. It takes the whole family, not just the two new members: leaving eleven
# siblings behind would make "where does a per-file .tg target live?" a question
# with two answers, which is the drift the same ADR spent its P2 removing.
#
# The blanket gate `tg-test` and the LLVM/codegen suites stay in quality.mk —
# they are gates, and this file is deliberately not.

.PHONY: tg-test-closures tg-test-cir tg-test-if-let tg-test-try-block \
	tg-test-type-alias tg-test-string-concat tg-test-pattern tg-test-list-ops \
	tg-test-compare-result tg-test-string-utils tg-test-int-typing tg-test-int-ops

## Run .tg closure emitter tests (ADR 13.5.26m) — cost 5, needs LLVM
tg-test-closures:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_codegen_closures.tg

## Run .tg CIR capture list tests (ADR 13.5.26j) — cost 5
tg-test-cir:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_cir_captures.tg

## Run if-let .tg tests (ADR 14.5.26e) — cost 3 (expect_type) + cost 5 (assert_eq_nat)
tg-test-if-let:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test tests/if_let.tg

## THE COST TIER IS NOT WRITTEN HERE (ADR 6.8.26c D4). Five of the targets
## below used to pass `--check-only` inline while `tg-test`'s blanket loop
## passed only `--require-tests` — the same file, two modes, one of them
## declared and ignored, which reported 19 correct-by-design cost-3 tests as
## ASSERTED NOTHING. The tier now lives in `tg-test-tiers.toml`, which the
## RUNNER resolves, so a bare `tungsten test <file>` behaves identically here
## and under the gate. Re-adding an inline `--check-only` re-creates the drift,
## and `test_runner::tier`'s `no_recipe_line_declares_a_cost_tier_inline` test
## fails on one.

## Run try-block .tg tests (ADR 15.5.26d) — cost 3 (expect_type/expect_error)
tg-test-try-block:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test tests/try_block.tg

## Run type-alias .tg tests (ADR 15.5.26g) — cost 3 (expect_type/expect_error)
tg-test-type-alias:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test tests/type_alias.tg

## Run string concat .tg tests (ADR 18.5.26f, 20.8.26d) — cost 5 (16 runtime assert_eq_string)
tg-test-string-concat:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_string_concat.tg

## Run nested pattern .tg tests (ADR 20.5.26a) — cost 3 (expect_type)
tg-test-pattern:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test tests/pattern_nested_tuple.tg

## Run list ops .tg tests (ADR 20.5.26e) — cost 3 (typed let-bindings)
tg-test-list-ops:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_list_ops.tg

## Run CompareResult ADT .tg tests (ADR 29.6.26f T11.1) — cost 3 (expect_type)
tg-test-compare-result:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_compare_result.tg

## Run string utils .tg tests — cost 5 (assert_eq_string / assert_eq_nat)
tg-test-string-utils:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_string_utils.tg

## Run Int typing .tg tests (ADR 14.9.26c AC 1) — cost 3 (expect_type / expect_error)
tg-test-int-typing:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test tests/int_typing.tg

## Run Int runtime .tg tests (ADR 14.9.26c AC 2) — cost 5 (assert_eq_int); the
## must-fail twin mustfail_int_ops.tg is covered by check-mustfail-fixtures
tg-test-int-ops:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_int_ops.tg --assertion-census


## Help for this category. `help` in the root Makefile composes the per-file
## `help-<category>` targets, so these lines live beside the targets they
## describe rather than in quality.mk — the two-copy split that let
## `tg-test-compare-result` and `tg-test-string-utils` ship undiscoverable
## (ADR 6.8.26c close-out review).
.PHONY: help-tg-suites
help-tg-suites:
	@echo ""
	@echo "Per-file .tg suites:"
	@echo "  make tg-test-closures   - Run closure emitter .tg tests (ADR 13.5.26m)"
	@echo "  make tg-test-cir        - Run CIR capture list .tg tests (ADR 13.5.26j)"
	@echo "  make tg-test-if-let     - Run if-let .tg tests (ADR 14.5.26e)"
	@echo "  make tg-test-try-block  - Run try-block .tg tests (ADR 15.5.26d)"
	@echo "  make tg-test-type-alias - Run type-alias .tg tests (ADR 15.5.26g)"
	@echo "  make tg-test-string-concat  - Run string concat .tg tests (ADR 18.5.26f)"
	@echo "  make tg-test-string-utils   - Run string utils .tg tests"
	@echo "  make tg-test-pattern    - Run nested pattern .tg tests (ADR 20.5.26a)"
	@echo "  make tg-test-list-ops   - Run list ops .tg tests (ADR 20.5.26e)"
	@echo "  make tg-test-compare-result - Run CompareResult ADT .tg tests (ADR 29.6.26f)"
	@echo "  make tg-test-int-typing - Run Int typing .tg tests (ADR 14.9.26c)"
	@echo "  make tg-test-int-ops    - Run Int runtime .tg tests (ADR 14.9.26c)"
	@echo "  Cost tiers are declared in tg-test-tiers.toml, not here (ADR 6.8.26c)"

HELP_SECTIONS += tg-suites
