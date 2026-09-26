# make/quality/ir-audits.mk — IR determinism, cache and emission audits
#
# The host-side canaries plus the `doctor check ir` audit family run over
# one freshly emitted corpus of the self-hosted compiler's own IR.

.PHONY: check-ir-determinism-canary check-cache-poisoning-canary check-ir-fingerprint-canary update-ir-fingerprint check-extern-map-ambiguity check-ir-audits check-indirect-buffers check-lowering-consistency check-arena-mode check-arena-mode-scan check-arena-branch-cost check-int-parity

## Fast IR determinism canary (host-side, ~3s) — ADR 18.5.26b
## Compiles a small .tg file twice with --emit-llvm and diffs the output.
check-ir-determinism-canary:
	@bash scripts/check-ir-determinism-canary.sh

## Cache-poisoning canary (ADR 4.7.26d) — host-side, no LLVM, ~2s.
##
## Runs each fixture COLD (fresh isolated cache) then WARM (reusing it) and
## compares exit status + stdout. The 4.7.26c bug was exactly this divergence:
## a bodyless signature-cache entry read back by `run`/`test`, surfacing as
## "no tests found" or a spurious E0030 — silent, and invisible to every other
## gate. `--gate` makes a divergence exit non-zero.
##
## Both modes are covered on purpose: 4.7.26c bit `test` (empty def list), and
## `run` is the mode the playground/evaluator path uses. Fixtures are the ones
## that already back diff_exec_e2e, so they stay maintained; the StringBuilder
## fixture (ADR 14.9.26a) rides along so the six builder externs are run
## cold-then-warm on the evaluator too.
##
## NOTE: this is the cold-vs-warm class only. It does NOT catch a cache written
## by one compiler build and read by a NEWER one — the validity key carries
## CARGO_PKG_VERSION, which is constant across edits (see
## docs/repo-memory/common-pitfalls.md "Why it goes stale silently"). For that
## class the mitigation stays `tungsten cache clean`.
check-cache-poisoning-canary:
	@echo "=== Cache-poisoning canary (cold vs warm) ==="
	$(CARGO) run -q -p tungsten_bootstrap --no-default-features -- \
	  diff cache tests/console_println_run.tg --mode run --gate
	$(CARGO) run -q -p tungsten_bootstrap --no-default-features -- \
	  diff cache tests/string_builder_run.tg --mode run --gate
	$(CARGO) run -q -p tungsten_bootstrap --no-default-features -- \
	  diff cache tests/dead_arm_letelse_run.tg --mode test --gate

## Colliding-name extern-map misresolution audit (ADR 12.7.26b) — cost 4,
## codegen feature, no LLVM emission. Exit non-zero on any ambiguous
## (unit, name) reference — the CI regression gate for ADR 12.7.26a.
check-extern-map-ambiguity:
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- doctor check codegen extern-map-ambiguity $(COMPILER_MAIN)

## IR audits over a REAL corpus (ADRs 17.7.26e §6.6, 28.7.26e) — the standing
## oracles for the `doctor check ir` family. Cost 4, requires codegen.
##
## Every one of these audits existed for months and was only ever run by hand,
## so their unit fixtures drifted from the shape the compiler actually emits
## while their tests stayed green. Pointing them at the self-hosted compiler's
## own IR found THREE of six red — `indirect-buffers` with 32 false escapes
## (17.7.26e), `null-calls` with 15 (`@cstring_is_null(` matched `null(`), and
## `declares` with 5 (LLVM IR text living in `@str_lit` constants) — plus one
## false `wrapper-self-calls` finding. An audit nobody runs is not a safety net;
## it is a file that compiles.
##
## Both targets emit IR for $(COMPILER_MAIN) into a scratch dir and audit it in
## --strict mode, where a vacuous pass — candidates found, nothing parsed — is a
## failure (exit 2), so a rename in the emitter cannot quietly empty an audit's
## candidate set. The emit dominates the cost (~24 s host-side for ~2,055 `.ll`),
## so auditing six times over one corpus is near-free.
##
## The dir is re-emitted on every invocation and never cached across runs: a
## corpus that outlived a compiler rebuild would make the gate green over IR the
## compiler no longer emits — this ADR's own failure mode, reintroduced as a
## cost optimization.
IR_AUDIT_LL_DIR ?= target/ll-audit
# Retained alias: ADR 17.7.26e named this after the one audit that used it.
INDIRECT_BUFFER_LL_DIR ?= $(IR_AUDIT_LL_DIR)

## Emit the audit corpus. Both audit targets depend on this recipe, so a change
## to the emit flags reaches both (there is one definition, two entry points).
define emit-ir-audit-corpus
	@rm -rf $(IR_AUDIT_LL_DIR)
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- \
	  compile --emit-llvm $(COMPILER_MAIN) -o $(IR_AUDIT_LL_DIR)
endef

## The audit driver. A single-line variable, not a `define`: a canned recipe's
## embedded newline would split the aggregating shell below into one shell per
## audit, and `exit $$rc` would then run with `rc` unset — a gate that reports
## "✗ failed" and exits 0.
IR_AUDIT_CMD = $(CARGO) run -q -p tungsten_bootstrap --no-default-features -- doctor check ir

## All six dir-scanning IR audits over one freshly emitted corpus (ADR 28.7.26e
## D1). Every audit runs as `… || rc=1` so make's abort-on-first-failure does
## not hide the rest: a developer who breaks two audits should learn both in one
## run. Exits non-zero iff any audit is red.
check-ir-audits:
	@echo "=== IR audits over the self-hosted compiler's emitted IR (emit once, audit six) ==="
	$(emit-ir-audit-corpus)
	@rc=0; \
	$(IR_AUDIT_CMD) declares $(IR_AUDIT_LL_DIR) --strict || rc=1; \
	$(IR_AUDIT_CMD) null-calls $(IR_AUDIT_LL_DIR) --strict || rc=1; \
	$(IR_AUDIT_CMD) indirect-buffers $(IR_AUDIT_LL_DIR) --strict || rc=1; \
	$(IR_AUDIT_CMD) sret-stores $(IR_AUDIT_LL_DIR) --strict || rc=1; \
	$(IR_AUDIT_CMD) merge-truncation $(IR_AUDIT_LL_DIR) --strict || rc=1; \
	$(IR_AUDIT_CMD) wrapper-self-calls $(IR_AUDIT_LL_DIR) --strict || rc=1; \
	if [ $$rc -ne 0 ]; then echo "✗ one or more IR audits failed"; else echo "✓ all six IR audits clean and non-vacuous"; fi; \
	exit $$rc

## Single-audit fast path for ABI work — what an indirect-param change wants.
## Shares check-ir-audits' emit recipe and corpus dir; superseded by it in CI.
check-indirect-buffers:
	@echo "=== Class-P indirect-buffer audit (emit + --strict) ==="
	$(emit-ir-audit-corpus)
	$(CARGO) run -q -p tungsten_bootstrap --no-default-features -- \
	  doctor check ir indirect-buffers $(IR_AUDIT_LL_DIR) --strict

## Assert every non-recursive ADT lowers identically via every route — the
## named-vs-structural split-brain gate (ADR 12.7.26c P3). Cost 4, requires
## codegen; kept OFF check-health's no-codegen hot path as a dedicated target.
check-lowering-consistency:
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- doctor check type lowering-consistency $(COMPILER_MAIN)

## Int parity + trap gate (ADR 14.9.26c AC 3). Cost 5, requires codegen
## (host LLVM). Two `diff exec` runs over two fixtures:
##   1. tests/int_parity_run.tg — every Int node, printed; the evaluator's
##      checked arms and the native intrinsics/sdiv/srem must agree on stdout.
##   2. tests/int_overflow_trap_run.tg — prints, then overflows; `diff exec`
##      classifies both sides failing at runtime with equal stdout as parity,
##      so this passes only if BOTH sides trap after the print. A backend that
##      wrapped would print a third line and diverge.
## The static library is rebuilt first: the trap block calls `tg_int_trap`,
## which a stale `libtungsten_core.a` lacks.
check-int-parity:
	@echo "=== Int parity (evaluator vs native) + overflow-trap gate (ADR 14.9.26c) ==="
	$(CARGO) build -q -p tungsten_core
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- \
	  diff exec tests/int_parity_run.tg
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- \
	  diff exec tests/int_overflow_trap_run.tg
	@echo "✓ Int parity holds and the overflow trap fires on both sides"

## Compare canary IR output against stored fingerprint baseline (ADR 18.5.26d)
check-ir-fingerprint-canary:
	@bash scripts/check-ir-fingerprint.sh

## Update canary IR fingerprint baseline (ADR 18.5.26d)
update-ir-fingerprint:
	@bash scripts/check-ir-fingerprint.sh --update

## Arena-mode parity + callee gate (ADR 14.9.26b AC 5 / AC 7, amended by ADR
## 18.9.26c AC 5). Cost 5, requires codegen (host LLVM). Two checks over one
## fixture:
##   1. `diff exec` under TUNGSTEN_ARENA=bump:1 — the native side bump-
##      allocates from 1 MiB chunks (the fixture forces an oversized chunk and
##      a rollover) and must print what the evaluator prints. The variable
##      reaches only the compiled child: the bootstrap never calls
##      `__tungsten_arena_init` and stays `off`.
##   2. the non-profiled IR the bootstrap emits for the fixture loads
##      `@__tungsten_arena_mode`, calls `@__tungsten_alloc`, and calls `@malloc`
##      ONLY inside an `alloc.malloc[N]` block — the off arm of the mode branch.
##      `grep` cannot see block membership, so ARENA_MALLOC_SCAN remembers the
##      last label of each `.ll` and flags a `@malloc` call under any other.
## The static library is rebuilt first: the linker resolves
## `target/debug/libtungsten_core.a`, and a stale one lacks the symbols.
ARENA_MODE_FIXTURE ?= tests/arena_mode_run.tg
ARENA_MODE_LL_DIR ?= target/ll-arena-mode
## Exit 1 naming each `@malloc` call outside an `alloc.malloc[N]` block.
ARENA_MALLOC_SCAN = awk 'FNR == 1 || /^define / { label = "" } /^[^ ;\t]+:/ { label = $$1 } /call ptr @malloc\(/ && label !~ /^alloc\.malloc[0-9]*:$$/ { print "  " FILENAME ":" FNR ": @malloc call in block " (label == "" ? "<entry>" : label); bad = 1 } END { exit bad }'
check-arena-mode:
	@echo "=== Arena-mode parity (TUNGSTEN_ARENA=bump:1) + allocation callee gate ==="
	$(CARGO) build -q -p tungsten_core
	TUNGSTEN_ARENA=bump:1 $(CARGO) run -q -p tungsten_bootstrap --features codegen -- \
	  diff exec $(ARENA_MODE_FIXTURE)
	@rm -rf $(ARENA_MODE_LL_DIR)
	$(CARGO) run -q -p tungsten_bootstrap --features codegen -- \
	  compile --emit-llvm $(ARENA_MODE_FIXTURE) -o $(ARENA_MODE_LL_DIR)
	@$(MAKE) -s check-arena-mode-scan ARENA_MODE_LL_DIR=$(ARENA_MODE_LL_DIR)

## The IR half of check-arena-mode alone, over an existing `.ll` tree — no
## emit, so AC 5's red-arm drill can point it at a planted copy:
##   make check-arena-mode-scan ARENA_MODE_LL_DIR=<dir with a stray @malloc>
check-arena-mode-scan:
	@if ! grep -rq "call ptr @__tungsten_alloc(" $(ARENA_MODE_LL_DIR); then \
	  echo "✗ no @__tungsten_alloc call in the emitted IR"; exit 1; fi; \
	if ! grep -rq "load i32, ptr @__tungsten_arena_mode" $(ARENA_MODE_LL_DIR); then \
	  echo "✗ no @__tungsten_arena_mode load in the emitted IR"; exit 1; fi; \
	if ! $(ARENA_MALLOC_SCAN) $$(find $(ARENA_MODE_LL_DIR) -name '*.ll' | sort); then \
	  echo "✗ a @malloc call outside an alloc.malloc block"; exit 1; fi; \
	echo "✓ arena-mode parity holds; @malloc is called only on the mode-off arm"

## Three-variant same-IR A/B of the allocation-site mode branch (ADR 18.9.26c
## AC 1). IN-CONTAINER (hyperfine, llvm-link, opt, llc, the release build):
##   devcontainer exec --workspace-folder . make check-arena-branch-cost
## `closure_chain` is emitted once and linked into ONE `.ll`; three binaries
## are built from it by rewriting only the `@__tungsten_arena_mode` declaration:
##   emitted  `external hidden global i32` — this ADR's branch
##   0        `internal constant i32 0` — folds to the bare pre-14.9.26b @malloc
##   1        `internal constant i32 1` — folds to the parent's @__tungsten_alloc
## All three take the same `opt -passes=instcombine,simplifycfg` then `llc -O2`
## (llc alone will not reliably fold a load from a constant global) and the same
## link line; a grep proves each fold. `internal` never clashes with the
## runtime's real export, which `__tungsten_alloc` and the prologue keep reading.
## `--termination=report` is the benchmark harness's own workload flag
## (tools/tungsten-bench, WORKLOAD_TERMINATION_LEVEL): the workloads loop on Nat.
## Recorded, not gated (AC 1: *benchmark, recorded*) — the rows go into
## benchmarks/results/18.9.26c.arena-branch-cost.md.
ARENA_BRANCH_DIR ?= target/arena-branch-cost
ARENA_BRANCH_BENCH ?= benchmarks/closures/closure_chain.tg
ARENA_BRANCH_RELEASE = $${CARGO_TARGET_DIR:-target}/release
ARENA_BRANCH_LINK = -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc -Wl,-z,stack-size=134217728
check-arena-branch-cost:
	@echo "=== Arena branch cost: three-variant same-IR A/B on closure_chain (ADR 18.9.26c) ==="
	@set -e; d=$(ARENA_BRANCH_DIR); rm -rf $$d; mkdir -p $$d; \
	$(ARENA_BRANCH_RELEASE)/tungsten compile --termination=report --emit-llvm $(ARENA_BRANCH_BENCH) -o $$d/ll; \
	llvm-link -S $$(find $$d/ll -name '*.ll' | sort) -o $$d/emitted.ll; \
	decl='@__tungsten_arena_mode = external hidden global i32'; \
	grep -qx "$$decl" $$d/emitted.ll || { echo "✗ no '$$decl' line in the linked IR"; exit 1; }; \
	for v in emitted 0 1; do \
	  src=$$d/emitted.ll; \
	  if [ $$v != emitted ]; then src=$$d/v$$v.ll; \
	    sed "s/^$$decl\$$/@__tungsten_arena_mode = internal constant i32 $$v/" $$d/emitted.ll > $$src; fi; \
	  opt -S -passes=instcombine,simplifycfg $$src -o $$d/$$v.opt.ll; \
	  llc -O2 -filetype=obj $$d/$$v.opt.ll -o $$d/$$v.o; \
	  cc $$d/$$v.o -o $$d/closure_chain.$$v $(ARENA_BRANCH_RELEASE)/libtungsten_core.a $(ARENA_BRANCH_LINK); \
	  [ "$$($$d/closure_chain.$$v)" = "$$(cat benchmarks/closures/closure_chain.expected)" ] || { echo "✗ variant $$v printed the wrong answer"; exit 1; }; \
	done; \
	! grep -q "load i32, ptr @__tungsten_arena_mode" $$d/0.opt.ll || { echo "✗ variant 0 kept the mode load"; exit 1; }; \
	! grep -q "call ptr @__tungsten_alloc(" $$d/0.opt.ll || { echo "✗ variant 0 kept a @__tungsten_alloc call"; exit 1; }; \
	! grep -q "call ptr @malloc(" $$d/1.opt.ll || { echo "✗ variant 1 kept a @malloc call"; exit 1; }; \
	echo "✓ three variants built from one .ll, each fold verified"; \
	hyperfine --warmup 5 --runs 40 --export-markdown $$d/ab.md --export-csv $$d/ab.csv \
	  -n 'emitted off' "$$d/closure_chain.emitted" \
	  -n '0 (bare malloc)' "$$d/closure_chain.0" \
	  -n '1 (parent symbol) off' "$$d/closure_chain.1" \
	  -n 'emitted bump' "TUNGSTEN_ARENA=bump $$d/closure_chain.emitted" \
	  -n '1 (parent symbol) bump' "TUNGSTEN_ARENA=bump $$d/closure_chain.1"; \
	awk -F, 'NR > 1 { m[NR - 1] = $$2 } END { \
	  printf "emitted/0 off: %+.1f%%   1/emitted off: %+.1f%%   1/0 off: %+.1f%%   emitted/1 bump: %+.1f%%\n", \
	    (m[1] / m[2] - 1) * 100, (m[3] / m[1] - 1) * 100, (m[3] / m[2] - 1) * 100, (m[4] / m[5] - 1) * 100 }' $$d/ab.csv; \
	echo "Table: $$d/ab.md"
