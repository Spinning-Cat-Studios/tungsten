# make/compiler.mk — Self-hosted compiler check targets
#
# Commands for type-checking the self-hosted compiler source.

.PHONY: check-compiler check-lexer check-parser check-modules

# Check that the self-hosted compiler type-checks
check-compiler:
	@echo "Checking self-hosted compiler..."
	@rm -rf src/compiler/.tungsten src/compiler/**/.tungsten
	@cargo run -p tungsten_bootstrap --no-default-features -- check src/compiler/main.tg
	@echo "✓ Self-hosted compiler type-checks successfully"

# Check lexer module only
check-lexer:
	@echo "Checking lexer module..."
	@rm -rf src/compiler/.tungsten src/compiler/**/.tungsten
	@cargo run -p tungsten_bootstrap --no-default-features -- check src/compiler/lexer/mod.tg
	@echo "✓ Lexer module type-checks successfully"

# Check parser module only (requires main.tg context for lexer access)
check-parser:
	@echo "Checking parser module..."
	@rm -rf src/compiler/.tungsten src/compiler/**/.tungsten
	@cargo run -p tungsten_bootstrap --no-default-features -- check src/compiler/parser/mod.tg
	@echo "✓ Parser module type-checks successfully"

# Check module system integration tests
check-modules:
	@echo "Checking module integration tests..."
	@cargo run -p tungsten_bootstrap --no-default-features -- check tests/module_bugs/lexer_parser_pattern/main.tg
	@echo "✓ Module integration tests pass"

# Help section for compiler checks
.PHONY: help-compiler
help-compiler:
	@echo ""
	@echo "Self-Hosted Compiler:"
	@echo "  make check-compiler  - Type-check the self-hosted compiler"
	@echo "  make check-lexer     - Type-check lexer module only"
	@echo "  make check-modules   - Run module integration tests"
