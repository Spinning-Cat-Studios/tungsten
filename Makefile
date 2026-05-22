# Tungsten Makefile
# ==================
#
# Modular build system for the Tungsten project.
# Each concern area is in its own .mk file under make/.
# See ADR 18.4.26f for design rationale.

include make/config.mk
include make/core.mk
include make/usage.mk
include make/examples.mk
include make/compiler.mk
include make/native.mk
include make/devcontainer.mk
include make/diagnostics.mk
include make/profiling.mk
include make/publishing.mk
include make/quality.mk

# Default target
.PHONY: help
help:
	@echo "Tungsten Development Commands"
	@echo "=============================="
	@$(MAKE) -s help-core
	@$(MAKE) -s help-usage
	@$(MAKE) -s help-compiler
	@$(MAKE) -s help-native
	@$(MAKE) -s help-devcontainer
	@$(MAKE) -s help-diagnostics
	@$(MAKE) -s help-profiling
	@$(MAKE) -s help-publishing
	@$(MAKE) -s help-quality
