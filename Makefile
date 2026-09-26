# Tungsten Makefile
# ==================
#
# Modular build system for the Tungsten project.
# Each concern area is in its own .mk file under make/.
# See ADR 18.4.26f for design rationale.
#
# The files included below are the public build: they ship with the repository.
# Development tooling that drives crates or notes that do not ship is pulled in
# by ONE optional include at the end, so a checkout without it still has a
# working build, test and help (ADR 25.9.26l D5).

include make/config.mk
include make/core.mk
include make/usage.mk
include make/examples.mk
include make/compiler.mk
include make/native.mk
include make/devcontainer.mk
include make/diagnostics.mk
include make/quality.mk
include make/tg-suites.mk
include make/maintenance.mk

-include make/private.mk

# Staging carries its own release operator targets. This file is copied to
# staging but omitted from the public allowlist; the private checkout uses its
# own make/private.mk instead.
ifeq ($(wildcard make/private.mk),)
-include make/staging.mk
endif

# Default target. Each included file appends its section to HELP_SECTIONS, so an
# absent optional file drops its sections instead of failing `help`.
.PHONY: help
help:
	@echo "Tungsten Development Commands"
	@echo "=============================="
	@for section in $(HELP_SECTIONS); do $(MAKE) -s help-$$section || exit 1; done

## Fail when the optional include above did not load (ADR 25.9.26l D5): a
## mistyped `-include` is silent by construction, and every private target —
## check-health among them — would quietly vanish. `check-health` runs this.
.PHONY: check-private-make
check-private-make:
	@test -f make/private.mk || { echo "✗ make/private.mk is missing"; exit 1; }
	@test -n "$(PRIVATE_MAKE_LOADED)" || { echo "✗ make/private.mk exists but the Makefile did not load it"; exit 1; }
	@echo "✓ make/private.mk loaded"
