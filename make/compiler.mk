# make/compiler.mk — Self-hosted compiler check targets
#
# Commands for type-checking the self-hosted compiler source.

.PHONY: check-compiler check-codegen check-lexer check-parser check-modules

# Type-check the bootstrap crate WITH the codegen feature (ADR 13.7.26a D1/AC4).
# The codegen feature links LLVM 18, so it needs LLVM_SYS_180_PREFIX — which
# make/config.mk already exports. Use this instead of hand-typing
# `export LLVM_SYS_180_PREFIX=... && cargo check -p tungsten_bootstrap --features codegen`:
# the bare form is what the guardrail's narrow `export` allowlist exists to
# tolerate, but a target is the ergonomic path and cannot drift from the prefix
# config.mk computes (brew --prefix llvm@18, with a fallback).
#
# This is `cargo check` only — it does NOT produce a codegen-featured binary. To
# undo host-side feature clobbering (a make target rebuilding target/debug/tungsten
# WITHOUT codegen, so `tungsten compile` vanishes), you still need the build form
# named in CLAUDE.md § Host-side feature clobbering.
check-codegen:
	@echo "Type-checking tungsten_bootstrap with the codegen feature (LLVM 18)..."
	@$(CARGO) check -p tungsten_bootstrap --features codegen
	@echo "✓ Codegen-featured bootstrap type-checks successfully"

# Check that the self-hosted compiler type-checks
check-compiler:
	@echo "Checking self-hosted compiler..."
	@rm -rf src/compiler/.tungsten src/compiler/**/.tungsten
	@$(CARGO) run -p tungsten_bootstrap --no-default-features -- check src/compiler/main.tg
	@echo "✓ Self-hosted compiler type-checks successfully"

# Check lexer module only
check-lexer:
	@echo "Checking lexer module..."
	@rm -rf src/compiler/.tungsten src/compiler/**/.tungsten
	@$(CARGO) run -p tungsten_bootstrap --no-default-features -- check src/compiler/lexer/mod.tg
	@echo "✓ Lexer module type-checks successfully"

# Check parser module only (requires main.tg context for lexer access)
check-parser:
	@echo "Checking parser module..."
	@rm -rf src/compiler/.tungsten src/compiler/**/.tungsten
	@$(CARGO) run -p tungsten_bootstrap --no-default-features -- check src/compiler/parser/mod.tg
	@echo "✓ Parser module type-checks successfully"

# Check module system integration tests
check-modules:
	@echo "Checking module integration tests..."
	@$(CARGO) run -p tungsten_bootstrap --no-default-features -- check tests/module_bugs/lexer_parser_pattern/main.tg
	@echo "✓ Module integration tests pass"

# Help section for compiler checks
.PHONY: help-compiler
help-compiler:
	@echo ""
	@echo "Self-Hosted Compiler:"
	@echo "  make check-compiler  - Type-check the self-hosted compiler"
	@echo "  make check-codegen   - Type-check bootstrap with the codegen feature (LLVM 18)"
	@echo "  make check-lexer     - Type-check lexer module only"
	@echo "  make check-modules   - Run module integration tests"

HELP_SECTIONS += compiler
