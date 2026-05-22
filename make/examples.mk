# make/examples.mk — Example running and golden test targets
#
# Commands for running examples and verifying golden test output.

.PHONY: run-examples check-examples check-golden update-golden

# Run all examples
run-examples:
	@echo "=== hello.tg ===" && cargo run -p tungsten_bootstrap -- examples/hello.tg
	@echo "=== answer.tg ===" && cargo run -p tungsten_bootstrap -- examples/answer.tg
	@echo "=== arithmetic.tg ===" && cargo run -p tungsten_bootstrap -- examples/arithmetic.tg
	@echo "=== logic.tg ===" && cargo run -p tungsten_bootstrap -- examples/logic.tg
	@echo "=== proof.tg ===" && cargo run -p tungsten_bootstrap -- examples/proof.tg

# Check all examples type-check
check-examples:
	@cargo run -p tungsten_bootstrap -- check examples/hello.tg
	@cargo run -p tungsten_bootstrap -- check examples/answer.tg
	@cargo run -p tungsten_bootstrap -- check examples/arithmetic.tg
	@cargo run -p tungsten_bootstrap -- check examples/logic.tg
	@cargo run -p tungsten_bootstrap -- check examples/proof.tg

# Run golden tests (compare output with expected)
check-golden:
	@./tests/golden/run_golden.sh

# Update golden test expected files from bootstrap output
update-golden:
	@./tests/golden/run_golden.sh --update

# Help section for examples
.PHONY: help-examples
help-examples:
	@echo ""
	@echo "Examples:"
	@echo "  make run FILE=examples/hello.tg"
	@echo "  make eval EXPR='2 + 2'"
