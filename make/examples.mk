# make/examples.mk — Example running and golden test targets
#
# Commands for running examples and verifying golden test output.

.PHONY: run-examples check-examples

# Run all examples
run-examples:
	@echo "=== hello.tg ===" && cargo run -p tungsten_bootstrap -- examples/hello.tg
	@echo "=== answer.tg ===" && cargo run -p tungsten_bootstrap -- examples/answer.tg
	@echo "=== arithmetic.tg ===" && cargo run -p tungsten_bootstrap -- examples/arithmetic.tg
	@echo "=== logic.tg ===" && cargo run -p tungsten_bootstrap -- examples/logic.tg
	@echo "=== proof.tg ===" && cargo run -p tungsten_bootstrap -- examples/proof.tg

# Check every shipped example type-checks. It named five of them until ADR
# 18.9.26i, and `examples/proofs_natural.tg` — shipped in every release archive
# and quoted by the website — sat refused by the termination gate unnoticed.
check-examples:
	@$(CARGO) build -q -p tungsten_bootstrap --no-default-features --bin tungsten
	@rc=0; for f in examples/*.tg examples/*/mod.tg; do ./target/debug/tungsten check "$$f" >/dev/null 2>&1 && echo "✓ $$f" || { echo "✗ $$f"; rc=1; }; done; exit $$rc

# Golden tests live in make/quality/tg-tests.mk as `golden` / `golden-update`, which run
# the `tools/golden` Rust crate. The `check-golden` / `update-golden` aliases
# that used to sit here ran a second, shell implementation; two names for one
# job is the drift that let the shell runner exit 0 on a failing suite for an
# unknown period (ADR 31.7.26b D1).

# Help section for examples
.PHONY: help-examples
help-examples:
	@echo ""
	@echo "Examples:"
	@echo "  make run FILE=examples/hello.tg"
	@echo "  make eval EXPR='2 + 2'"
