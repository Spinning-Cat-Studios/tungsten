# make/usage.mk — Check, run, eval, compile targets
#
# Day-to-day usage commands for running the bootstrap compiler on .tg files.

.PHONY: check run eval compile compile-run

# Type-check a file (no LLVM needed)
check:
ifndef FILE
	@echo "Usage: make check FILE=<file>"
	@echo "Example: make check FILE=examples/hello.tg"
else
	cargo run -p tungsten_bootstrap --no-default-features -- check $(FILE)
endif

# Run a file (no LLVM needed)
run:
ifndef FILE
	@echo "Usage: make run FILE=<file>"
	@echo "Example: make run FILE=examples/hello.tg"
else
	cargo run -p tungsten_bootstrap --no-default-features -- $(FILE)
endif

# Evaluate an expression (no LLVM needed)
eval:
ifndef EXPR
	@echo "Usage: make eval EXPR=<expr>"
	@echo "Example: make eval EXPR='2 + 2'"
else
	cargo run -p tungsten_bootstrap --no-default-features -- eval "$(EXPR)"
endif

# Compile to native binary (requires LLVM 18 installed locally)
compile:
ifndef FILE
	@echo "Usage: make compile FILE=<file> [OUT=<output>]"
	@echo "Example: make compile FILE=examples/hello.tg OUT=hello"
	@echo ""
	@echo "Note: Requires LLVM 18. Set LLVM_SYS_180_PREFIX or use devcontainer."
else
	cargo run -p tungsten_bootstrap --release -- compile $(FILE) $(if $(OUT),-o $(OUT),)
endif

# Compile and run a native binary (requires LLVM 18)
# Supports: make compile-run FILE=examples/hello.tg  OR  make compile-run examples/hello.tg
compile-run:
	$(eval _FILE := $(or $(FILE),$(filter %.tg,$(MAKECMDGOALS))))
	@if [ -z "$(_FILE)" ]; then \
		echo "Usage: make compile-run FILE=<file>"; \
		echo "   or: make compile-run <file>"; \
		echo "Example: make compile-run examples/hello.tg"; \
		echo ""; \
		echo "Note: Requires LLVM 18. Set LLVM_SYS_180_PREFIX or use devcontainer."; \
		exit 1; \
	fi && \
	BASENAME=$$(basename $(_FILE) .tg) && \
	cargo run -p tungsten_bootstrap --release -- compile $(_FILE) -o /tmp/$$BASENAME && \
	echo "Running /tmp/$$BASENAME..." && \
	/tmp/$$BASENAME

# Catch-all for .tg files used as positional arguments
%.tg:
	@true

# Help section for usage commands
.PHONY: help-usage
help-usage:
	@echo ""
	@echo "  make check FILE=<file>  - Type-check a file"
	@echo "  make run FILE=<file>    - Run a file (interpreted)"
	@echo "  make eval EXPR=<expr>   - Evaluate an expression"
	@echo ""
	@echo "Native Compilation (requires LLVM 18):"
	@echo "  make compile FILE=<file>      - Compile to native binary"
	@echo "  make compile-run FILE=<file>  - Compile and run"
