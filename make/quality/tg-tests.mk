# make/quality/tg-tests.mk — .tg suites run through the self-hosted compiler
#
# The tg-test gate and its per-suite variants, the runner-honesty and
# comparator non-vacuity gates, and the golden snapshot tests.

.PHONY: tg-test tg-test-module tg-test-codegen tg-test-classp tg-test-dead-arm tg-test-comparator-codegen check-test-runner check-ast-compare-nonvacuous check-comparator-repros

## Run all .tg unit tests in the self-hosted compiler (ADR 12.5.26b).
## File list corrected per ADR 2.7.26b T5b: main.tg discovers ZERO runnable
## tests (the vacuous green); the runnable suites are the src/compiler/test_*.tg
## entry files. --require-tests makes an empty discovery a hard failure.
##
## COST (ADR 5.8.26d, measured 2026-08-05, release binary, one 4-`mod` file).
## Each file's `mod` declarations elaborate the whole self-hosted compiler before
## its assertions run, so the gate is ~95% front-end re-elaboration, fifteen times
## over. The fixed cost is the `check` pipeline entire — `check` 38.01 s against
## `test` 38.62 s on the same file — of which Body Elaboration is 34.8 s, 90% of
## wall. Test discovery + execution is ~0.5 s, so speeding up the RUNNER
## addresses ~1% of this gate.
##
## MEASURED DEAD END — but NOT for the reason you will first reach for (D2).
## Warming the elaboration cache so files 2-15 read what file 1 wrote buys
## nothing here, and the reason is the cache KEY, not the cache. An entry is
## keyed on the GLOBAL exports hash (cache/elab_cache/mod.rs `hash_exports`,
## deliberately over-invalidating) and each entry file contributes its own
## `test_*` definitions to it. Measured: re-running the SAME file against its own
## warm cache is 38.62 s -> 3.58 s at 230/230 modules hit, a real 10.8x; running a
## DIFFERENT file against that cache is 0/230 hit and full cold cost (39.73 s).
## So the amortisation lever is ONE process elaborating ONE module graph for all
## fifteen suites — not TUNGSTEN_ELAB_CACHE_FULL=1 in front of this loop.
##
## The leading `cache clean` is LOAD-BEARING, not hygiene: it makes every run
## cold and uniform, which is what makes these timings comparable across runs and
## between developers. Do not remove it as part of a speed change.
##
## The `case` line is defence in depth (ADR 7.8.26a §2.3). The self-host `test`
## verb writes a harness beside the file under test and deletes it on no path;
## its old name `<base>.tungsten_test.tg` MATCHED this glob, so one leftover
## became an extra gate input carrying a whole duplicate compiler. §2.3 renamed
## it `_tgtest_*`; this line means no future writer can reopen the hole. Cheap,
## because an extra gate input that PASSES looks like a healthy gate.
tg-test:
	@set -e; \
	$(TG_GATE_BUILD); \
	$(TG_GATE_BIN) cache clean >/dev/null 2>&1 || true; \
	for f in src/compiler/test_*.tg; do \
		case "$$f" in *.tungsten_test.tg|*_tgtest_*) continue;; esac; \
		echo "[tg-test] $$f"; \
		$(TG_GATE_BIN) test "$$f" --require-tests; \
	done

## Prove the AST-comparison suite is NOT vacuous (ADR 29.6.26f §4.1).
##
## A passing `assert_eq` is not evidence that anything was compared: `compare` at
## a type whose comparator cannot be synthesized leaves the call Stuck, so the
## body's asserts never run and the test is reported `ok` — measured on the real
## `TypeExpr` for both `assert_eq(TyPath(..), TyUnit(..))` and `assert_ne(x, x)`.
## `mustfail_ast_compare.tg` mirrors — the wrong way round — every type the two
## positive suites assert on (`test_ast_compare.tg` self-contained,
## `test_ast_compare_cluster.tg` the mutually recursive cluster, ADR 1.8.26c).
check-ast-compare-nonvacuous:
	@set -e; \
	$(CARGO) build -q -p tungsten_bootstrap --no-default-features; \
	BIN=target/debug/tungsten; $$BIN cache clean >/dev/null 2>&1 || true; \
	OUT=$$($$BIN test src/compiler/mustfail_ast_compare.tg --require-tests 2>&1) || true; \
	P=$$(echo "$$OUT" | grep -oE '[0-9]+ passed' | head -1 | grep -oE '[0-9]+'); \
	F=$$(echo "$$OUT" | grep -oE '[0-9]+ failed' | head -1 | grep -oE '[0-9]+'); \
	if [ "$$P" != "0" ] || [ -z "$$F" ] || [ "$$F" -eq 0 ]; then \
		echo "[nonvacuous] ✗ every must-fail case must FAIL (got $$P passed, $$F failed):"; \
		echo "$$OUT" | grep -E '^test .* \.\.\. ok$$' || echo "$$OUT"; \
		echo "[nonvacuous]   those types are no longer compared; their positive tests assert nothing"; \
		exit 1; \
	fi; \
	echo "[nonvacuous] ✓ all $$F must-fail cases failed (suite is non-vacuous)"; \
	for p in src/compiler/test_ast_compare.tg src/compiler/test_ast_compare_cluster.tg; do \
		$$BIN test $$p --require-tests >/dev/null 2>&1 || { echo "[nonvacuous] ✗ $$p did not pass"; exit 1; }; \
		echo "[nonvacuous] ✓ $$p passes"; done

## Pin the comparator repro fixtures (ADR 1.8.26b D1/D2 — now CLOSED).
##
## Each returns a bit-packed vector where a set bit means the comparator wrongly
## called two DIFFERENT values Equal, so the correct output is `0`. Until
## 1.8.26b landed this asserted the opposite — that both still went Stuck — and
## said what to do when they stopped: "change this gate to require output 0".
check-comparator-repros:
	@set -e; \
	$(CARGO) build -q -p tungsten_bootstrap --no-default-features; \
	BIN=target/debug/tungsten; rc=0; \
	for f in tests/comparator_ctor_arity_repro.tg tests/comparator_mutual_mu_repro.tg; do \
		if OUT=$$($$BIN run $$f 2>/dev/null) && [ "$$OUT" = "0" ]; then \
			echo "[repros] ✓ $$f returns 0 (every differing pair reported unequal)"; \
		else \
			echo "[repros] ✗ $$f returned '$$OUT' (expected 0)."; \
			echo "[repros]   A nonzero bit means the comparator called two DIFFERING values Equal."; \
			echo "[repros]   A nonzero exit means it stopped comparing at all — see ADR 1.8.26b."; \
			rc=1; \
		fi; \
	done; \
	exit $$rc

## Verify the `tungsten test` runner consults the runtime failure flag (ADR 29.6.26f
## T13): a file with a deliberately-failing test must report FAILED and exit nonzero
## (before the fix every elaborated test was reported `ok`). Also confirms an all-pass
## file exits 0.
##
## Extended by ADR 6.8.26b's close-out with the two VACUITY outcomes, which are the
## same defect one layer down: consulting the failure flag is not enough, because an
## assertion that never runs never sets it. Each has its own fixture because each owns
## a case the other structurally cannot see — zero assertions (the count) versus
## some-but-not-all (the residual) — and the ORDER between them is what the pair pins:
## a wholly vacuous body arrives with a zero count AND a residual, so a residual-first
## rule would report it DID NOT FINISH and bury the more useful diagnosis.
check-test-runner:
	@set -e; \
	$(CARGO) build -q -p tungsten_bootstrap --no-default-features; \
	BIN=target/debug/tungsten; \
	$$BIN cache clean >/dev/null 2>&1 || true; rm -rf tests/.tungsten; \
	echo "[test-runner] a file with a failing test must exit nonzero + report FAILED"; \
	if OUT=$$($$BIN test tests/test_runner_pass_fail.tg 2>&1); then \
		echo "[test-runner] ✗ runner exited 0 despite a failing test:"; echo "$$OUT"; exit 1; \
	fi; \
	echo "$$OUT" | grep -q "test_deliberately_fails ... FAILED" || { echo "[test-runner] ✗ failing test not reported FAILED:"; echo "$$OUT"; exit 1; }; \
	echo "$$OUT" | grep -q "1 failed" || { echo "[test-runner] ✗ summary did not count the failure:"; echo "$$OUT"; exit 1; }; \
	echo "[test-runner] ✓ failing test → FAILED + nonzero exit"; \
	rm -rf tests/.tungsten; \
	echo "[test-runner] an all-pass file must exit 0"; \
	$$BIN test tests/test_runner_allpass.tg >/dev/null 2>&1 || { echo "[test-runner] ✗ all-pass file exited nonzero"; exit 1; }; \
	echo "[test-runner] ✓ all-pass file → exit 0"; \
	rm -rf tests/.tungsten; \
	echo "[test-runner] a test whose assertion never RUNS must exit nonzero + ASSERTED NOTHING"; \
	if OUT=$$($$BIN test tests/test_runner_asserted_nothing.tg --require-tests 2>&1); then \
		echo "[test-runner] ✗ vacuous test exited 0 — the count is not gating:"; echo "$$OUT"; exit 1; \
	fi; \
	echo "$$OUT" | grep -q "ASSERTED NOTHING" || { echo "[test-runner] ✗ not reported ASSERTED NOTHING:"; echo "$$OUT"; exit 1; }; \
	echo "$$OUT" | grep -q "1 asserted nothing" || { echo "[test-runner] ✗ summary did not count it:"; echo "$$OUT"; exit 1; }; \
	if echo "$$OUT" | grep -q "DID NOT FINISH"; then \
		echo "[test-runner] ✗ precedence inverted: zero assertions must outrank the residual check:"; echo "$$OUT"; exit 1; \
	fi; \
	echo "[test-runner] ✓ zero-assertion test → ASSERTED NOTHING + nonzero exit"; \
	rm -rf tests/.tungsten; \
	echo "[test-runner] a body that stops after asserting must exit nonzero + DID NOT FINISH"; \
	if OUT=$$($$BIN test tests/test_runner_did_not_finish.tg --require-tests 2>&1); then \
		echo "[test-runner] ✗ partially-vacuous test exited 0 — a nonzero count hid the residual:"; echo "$$OUT"; exit 1; \
	fi; \
	echo "$$OUT" | grep -q "DID NOT FINISH (after 1 assertion" || { echo "[test-runner] ✗ not reported DID NOT FINISH after 1 assertion:"; echo "$$OUT"; exit 1; }; \
	echo "$$OUT" | grep -q "1 did not finish" || { echo "[test-runner] ✗ summary did not count it:"; echo "$$OUT"; exit 1; }; \
	echo "[test-runner] ✓ partial vacuity → DID NOT FINISH + nonzero exit"; \
	rm -rf tests/.tungsten; \
	echo "[test-runner] --assertion-census must report the zero row AND the nonzero rows"; \
	OUT=$$($$BIN test tests/test_runner_asserted_nothing.tg --assertion-census 2>&1) || true; \
	echo "$$OUT" | grep -q "0 assertion(s) executed — this test proves nothing" || { echo "[test-runner] ✗ census did not call out the zero row:"; echo "$$OUT"; exit 1; }; \
	rm -rf tests/.tungsten; \
	OUT=$$($$BIN test tests/test_runner_allpass.tg --assertion-census 2>&1) || { echo "[test-runner] ✗ census on a clean file exited nonzero:"; echo "$$OUT"; exit 1; }; \
	echo "$$OUT" | grep -q "1 assertion(s) executed" || { echo "[test-runner] ✗ census did not report the nonzero rows:"; echo "$$OUT"; exit 1; }; \
	echo "[test-runner] ✓ census → zero row flagged, clean file exit 0"; \
	rm -rf tests/.tungsten; \
	echo "[test-runner] comparator ADT unit tests (14 test_* via the runner) must all pass"; \
	if OUT=$$($$BIN test tests/comparator_adt_tests.tg 2>&1); then \
		echo "$$OUT" | grep -q "0 failed" || { echo "[test-runner] ✗ some ADT tests failed:"; echo "$$OUT"; exit 1; }; \
		echo "[test-runner] ✓ comparator ADT tests pass ($$(echo "$$OUT" | grep -oE '[0-9]+ passed' | head -1))"; \
	else \
		echo "[test-runner] ✗ comparator ADT tests exited nonzero:"; echo "$$OUT"; exit 1; \
	fi; \
	rm -rf tests/.tungsten

## Run .tg unit tests scoped to a single module (ADR 12.5.26b)
## Usage: make tg-test-module MODULE=src/compiler/elab/env/mod.tg
tg-test-module:
ifndef MODULE
	$(error MODULE is required, e.g. make tg-test-module MODULE=src/compiler/elab/env/mod.tg)
endif
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test src/compiler/main.tg --module $(MODULE)

## Run .tg codegen emitter tests (ADR 13.5.26j) — cost 5, needs LLVM
tg-test-codegen:
	$(CARGO) run -p tungsten_bootstrap --no-default-features -- test src/compiler/test_codegen.tg

## Class-P indirect struct-param ABI regression (ADR 1.7.26e P2–P5) — cost 5, needs LLVM.
## Compiles + runs tests/classp_musttail_run.tg (deep self-recursion threading a
## non-flattenable struct param). Exit 0 iff every Class-P shape (scalar/sret+indirect/
## multi-param-swap/function-value) is O(1)-stack AND numerically correct — an O(N)-stack
## SKIP regression would segfault on the 5,000,000-deep spin loop.
tg-test-classp:
	@set -e; \
	echo "[classp] compile + run tests/classp_musttail_run.tg (expect exit 0 — O(1) stack)"; \
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- compile tests/classp_musttail_run.tg -o /tmp/classp_musttail_run; \
	/tmp/classp_musttail_run; \
	echo "[classp] compile + run tests/classp_sret_match_order_run.tg (expect exit 0 — ADR 1.7.26e §6.5 match-arm-order)"; \
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- compile tests/classp_sret_match_order_run.tg -o /tmp/classp_sret_match_order_run; \
	/tmp/classp_sret_match_order_run; \
	echo "[classp] ✓ Class-P musttail regressions passed (exit 0)"

## Dead-arm let-else regression (ADR 3.7.26a) — cost 5, needs LLVM.
## tests/dead_arm_letelse_run.tg mirrors elab/env/resolve.tg: a self-recursive
## list walk whose `let … else { return <recursive call> }` arm terminates
## control flow. A regressed compiler either rejects it (T1 shrinking-cast from
## the dead arm's ⊥ placeholder poisoning lambda return-type derivation) or
## miscompiles it (sret early return dropping the value) — both fail this gate:
## the binary must run and print 0 (value-correct on hit, miss, and probe-miss
## paths; compiled binaries always exit 0, so the gate asserts on stdout).
tg-test-dead-arm:
	@set -e; \
	echo "[dead-arm] compile + run tests/dead_arm_letelse_run.tg (expect output 0 — value-correct)"; \
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- compile tests/dead_arm_letelse_run.tg -o /tmp/dead_arm_letelse_run; \
	OUT=$$(/tmp/dead_arm_letelse_run); \
	if [ "$$OUT" != "0" ]; then echo "[dead-arm] ✗ value regression: main returned $$OUT (0=ok, 1=hit path, 2=miss path, 3=probe-miss path)"; exit 1; fi; \
	echo "[dead-arm] ✓ dead-arm let-else regression passed (output 0)"

## Structural-comparator codegen test (ADR 29.6.26f P6′ step 2 + P3) — needs LLVM.
## Verifies the codegen-time `__cmp<ConcreteT>` resolution end-to-end: the success
## program compiles + runs to exit 0 (all comparisons correct across leaves, tuples,
## Option, recursive List, records, nested records, nested Option<List>); the reject
## program (a `compare` on a function value) must FAIL compilation with the P3
## "not comparable" diagnostic.
tg-test-comparator-codegen:
	@set -e; \
	echo "[comparator-codegen] cleaning cache"; \
	$(CARGO) run -q -p tungsten_bootstrap --no-default-features -- cache clean >/dev/null 2>&1 || true; \
	echo "[comparator-codegen] success: compile + run tests/comparator_codegen_run.tg (expect exit 0)"; \
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- compile tests/comparator_codegen_run.tg -o /tmp/comparator_codegen_run; \
	/tmp/comparator_codegen_run; \
	echo "[comparator-codegen] ✓ success program ran (exit 0)"; \
	echo "[comparator-codegen] reject: compile tests/comparator_codegen_reject.tg (expect failure w/ 'not comparable')"; \
	if cargo run -q -p tungsten_bootstrap --features codegen -- compile tests/comparator_codegen_reject.tg -o /tmp/comparator_codegen_reject 2>/tmp/comparator_codegen_reject.err; then \
		echo "[comparator-codegen] ✗ reject program compiled but should have failed"; exit 1; \
	fi; \
	if grep -q "not comparable" /tmp/comparator_codegen_reject.err; then \
		echo "[comparator-codegen] ✓ reject program failed with P3 diagnostic"; \
	else \
		echo "[comparator-codegen] ✗ reject failed without the expected 'not comparable' diagnostic:"; \
		cat /tmp/comparator_codegen_reject.err; exit 1; \
	fi; \
	echo "[comparator-codegen] P4: compile + run tests/comparator_list100k_run.tg (100k list, expect exit 0 — O(1) stack)"; \
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- compile tests/comparator_list100k_run.tg -o /tmp/comparator_list100k; \
	/tmp/comparator_list100k; \
	echo "[comparator-codegen] ✓ P4 100k-list compared without stack overflow"; \
	echo "[comparator-codegen] P4: list path grammar (Index/Len) via evaluator (expect 15)"; \
	P4=$$(cargo run -q -p tungsten_bootstrap --no-default-features -- run tests/comparator_p4_paths_run.tg 2>/dev/null | tail -1); \
	if [ "$$P4" = "15" ]; then \
		echo "[comparator-codegen] ✓ P4 list paths correct ([i] element, .len length)"; \
	else \
		echo "[comparator-codegen] ✗ P4 list paths wrong: got $$P4, want 15"; exit 1; \
	fi; \
	echo "[comparator-codegen] AC8: source-level path grammar (.field/[index]/.tag/.N + canonical order) via evaluator (expect 127)"; \
	G=$$(cargo run -q -p tungsten_bootstrap --no-default-features -- run tests/comparator_path_grammar_run.tg 2>/dev/null | tail -1); \
	if [ "$$G" = "127" ]; then \
		echo "[comparator-codegen] ✓ AC8 path grammar correct (record .field by name, tuple [index], .tag, .N)"; \
	else \
		echo "[comparator-codegen] ✗ AC8 path grammar wrong: got $$G, want 127"; exit 1; \
	fi; \
	echo "[comparator-codegen] helpers: assert_some/assert_none/assert_ne + nested Option<List<(Nat,Bool)>> via evaluator (expect 0)"; \
	H=$$(cargo run -q -p tungsten_bootstrap --no-default-features -- run tests/comparator_helpers_run.tg 2>/dev/null | tail -1); \
	if [ "$$H" = "0" ]; then \
		echo "[comparator-codegen] ✓ P6 helpers pass (assert_some unwraps, assert_none, assert_ne, nested)"; \
	else \
		echo "[comparator-codegen] ✗ P6 helpers failed: got $$H, want 0"; exit 1; \
	fi; \
	echo "[comparator-codegen] P3/AC6: record with an opaque field must reject with the field path"; \
	if cargo run -q -p tungsten_bootstrap --features codegen -- compile tests/comparator_record_reject.tg -o /tmp/comparator_record_reject 2>/tmp/comparator_record_reject.err; then \
		echo "[comparator-codegen] ✗ record-opaque-field program compiled but should have failed"; exit 1; \
	fi; \
	if grep -q "not comparable" /tmp/comparator_record_reject.err && grep -q '\$$.run' /tmp/comparator_record_reject.err; then \
		echo "[comparator-codegen] ✓ record opaque field rejected with path (\$$.run)"; \
	else \
		echo "[comparator-codegen] ✗ record reject missing 'not comparable' or the \$$.run field path:"; \
		cat /tmp/comparator_record_reject.err; exit 1; \
	fi

## `golden` names a real crate directory (tools/golden), so without this make
## would resolve the target against the filesystem if one ever appeared at the
## repo root — the class of bug .PHONY exists for. Declared here rather than in
## the file's header block so it sits with the recipes it protects.
.PHONY: golden golden-update

## Run golden snapshot tests (all categories)
golden:
	$(CARGO) run --release -p golden

## Update golden snapshot expected files
golden-update:
	$(CARGO) run --release -p golden -- --update

.PHONY: check-mustfail-fixtures

## Every `src/compiler/mustfail_*.tg` must FAIL, and fail for its OWN reason.
##
## A must-fail fixture is a negative control: it asserts something false, or
## presents input a gate must reject, and its whole value is that it stays red.
## Nothing checked that. `mustfail_ast_compare.tg` had a bespoke gate
## (`check-ast-compare-nonvacuous`); `mustfail_test_discovery.tg` — added by ADR
## 7.8.26a to prove that skipped tests do NOT satisfy `--require-tests` — was
## verified once by hand and then run by nothing (found by that ADR's
## `/check-adr`). This target makes the CLASS discoverable rather than each
## instance: a new `mustfail_*.tg` with no entry in EXPECT below is an error,
## which is `tg-test-tiers.toml`'s `must_declare` discipline applied to fixtures.
##
## Exit != 0 alone is NOT the assertion, and that distinction is the whole
## design. A must-fail file also exits non-zero when it fails to parse, when a
## module it imports stops elaborating, or when the binary is missing — so a
## gate checking only the exit code goes green for reasons that have nothing to
## do with the invariant, which is precisely how a negative control rots into
## decoration. Each fixture therefore declares the SIGNATURE its failure must
## carry, in the `case` below, and the gate requires both.
## A fixture with a stronger bespoke gate is DELEGATED, not re-run: this target
## exists to cover the class, not to double the CI bill. `mustfail_ast_compare`
## already has `check-ast-compare-nonvacuous`, which asserts `0 passed` AND
## `N failed` AND that both positive suites still pass — strictly more than the
## signature check here, over a file that elaborates the whole compiler (~40 s).
## Delegation still satisfies the must-declare rule, so the file cannot silently
## become unowned if that gate is ever deleted.
check-mustfail-fixtures:
	@set -e; \
	$(TG_GATE_BUILD); \
	rc=0; n=0; d=0; \
	for f in src/compiler/mustfail_*.tg; do \
		case "$$(basename "$$f" .tg)" in \
		  mustfail_test_discovery) want='zero runnable tests discovered';; \
		  mustfail_string_builder) want='test_pushes_equal_concat_fold';; \
		  mustfail_int_ops) want='test_sub_is_not_saturating';; \
		  mustfail_int_match) want='test_or_arm_is_not_shadowed_by_the_guard';; \
		  mustfail_ast_compare) \
		     echo "[mustfail] → $$f delegated to check-ast-compare-nonvacuous"; \
		     d=$$((d + 1)); continue;; \
		  *) echo "[mustfail] ✗ $$f declares no expected failure signature in make/quality/tg-tests.mk"; \
		     echo "[mustfail]   a negative control nothing declares is a file nothing checks"; \
		     rc=1; continue;; \
		esac; \
		out=$$($(TG_GATE_BIN) test "$$f" --require-tests 2>&1) && code=0 || code=$$?; \
		n=$$((n + 1)); \
		if [ "$$code" -eq 0 ]; then \
			echo "[mustfail] ✗ $$f PASSED — a negative control that passes asserts nothing"; rc=1; \
		elif ! printf '%s' "$$out" | grep -q "$$want"; then \
			echo "[mustfail] ✗ $$f failed, but not for its own reason (wanted: $$want)"; \
			printf '%s\n' "$$out" | tail -5; rc=1; \
		else \
			echo "[mustfail] ✓ $$f fails as declared: $$want"; \
		fi; \
	done; \
	if [ "$$((n + d))" -eq 0 ]; then \
		echo "[mustfail] ✗ no mustfail_*.tg fixtures found — the glob matched nothing"; rc=1; fi; \
	if [ "$$rc" -eq 0 ]; then \
		echo "[mustfail] ✓ $$n fixture(s) red for the right reason, $$d delegated"; fi; \
	exit $$rc

