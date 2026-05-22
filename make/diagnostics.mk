# make/diagnostics.mk — Diagnostic tool targets
#
# Commands for running diagnostic tools on the self-hosted compiler.
# These wrap the bootstrap's --dump-core and --check-tyvar-escape flags.

.PHONY: dump-core check-tyvar-escape

# Dump Core IR for matching definitions (native Mac, release build)
dump-core:
ifndef PATTERN
	@echo "Usage: make dump-core PATTERN=<pattern>"
	@echo "Examples:"
	@echo "  make dump-core PATTERN=main"
	@echo "  make dump-core PATTERN='*'"
else
	cargo run -p tungsten_bootstrap --release -- check src/compiler/main.tg --dump-core "$(PATTERN)" --max-errors=0
endif

# Check TyVar escapes in the self-hosted compiler (native Mac, release build)
check-tyvar-escape:
	cargo run -p tungsten_bootstrap --release -- check src/compiler/main.tg --check-tyvar-escape --max-errors=0

# Help section for diagnostics
.PHONY: help-diagnostics
help-diagnostics:
	@echo ""
	@echo "Diagnostics:"
	@echo "  make dump-core PATTERN=<pat>   - Dump Core IR for matching defs"
	@echo "  make check-tyvar-escape        - Check TyVar escapes in compiler"
