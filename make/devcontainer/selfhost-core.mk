# make/devcontainer/selfhost-core.mk — what the SELF-HOST's Core terms look like
#
# Split out of devcontainer.mk by ADR 3.9.26h, the same way the x86_64 arm was
# by 31.7.26c: the file was at 398 of the 400-line `mk-size` cap, and these
# four targets are the one cohesive family in it — every one asks a question
# about the Core the self-hosted compiler produced, and every one needs a
# `-dev`/`-dev-fast` tungsten1 rather than the production build.
#
# `make help` still prints them from help-devcontainer, in devcontainer.mk.

.PHONY: devcontainer-dump-core devcontainer-check-tyvar-escape
.PHONY: devcontainer-selfhost-closed-terms devcontainer-selfhost-well-typed-terms

# Dump Core IR for matching definitions (self-host source, uses release build)
devcontainer-dump-core:
ifndef PATTERN
	@echo "Usage: make devcontainer-dump-core PATTERN=<pattern>"
	@echo "Examples:"
	@echo "  make devcontainer-dump-core PATTERN=main"
	@echo "  make devcontainer-dump-core PATTERN='*'"
else
	devcontainer exec --workspace-folder . bash -c '$(DC_TARGET)/release/tungsten check src/compiler/main.tg --dump-core "$(PATTERN)" --max-errors=0'
endif

# Closed-term gate for the self-hosted compiler's own output (ADR 21.8.26a).
#
# The regression guard the payload-projection fix needs, and the only one it
# CAN have: every wrong answer this class produces is a term nothing else
# reads. Codegen rebuilds the bindings from the pattern, so the self-compile
# stays green; the type checker resolves the name through the environment, so
# the check stays green. A missed lowering site is visible in exactly one
# place — the count of definitions whose Core term is not closed, which this
# holds at 0.
#
# Needs a `-dev-fast`/`-dev` tungsten1: the production build stubs the
# diagnostics out, and the check says so rather than reporting clean.
devcontainer-selfhost-closed-terms:
	devcontainer exec --workspace-folder . bash -c '$(DC_TARGET)/release/tungsten doctor check selfhost closed-terms src/compiler/main.tg'

# Shape gate for the same output (ADR 3.9.26h) — the SIBLING of the target
# above, and the one it cannot stand in for.
#
# `closed-terms` answers "is every name bound", and both defects that surfaced
# while closing 21.8.26a passed it: a constructor application whose curried
# arrow was paired with a unary lambda, and a tuple projection that took `fst`
# of a scalar. Both closed, both type-checking, both invisible to a green
# self-compile. Run BOTH — a term can fail either alone.
#
# SHRINK-ONLY, not a hard 0: 3.9.26h measured 300 of 2302 before 3.9.26e was
# fixed, so this fails on MORE than that AND on fewer — a fix means lowering
# `MAIN_TG_BASELINE` in the same commit.
#
# Needs a `-dev-fast`/`-dev` tungsten1, for the same reason.
devcontainer-selfhost-well-typed-terms:
	devcontainer exec --workspace-folder . bash -c '$(DC_TARGET)/release/tungsten doctor check selfhost well-typed-terms src/compiler/main.tg'

# Check TyVar escapes in the self-hosted compiler source (uses release build)
devcontainer-check-tyvar-escape:
	devcontainer exec --workspace-folder . bash -c '$(DC_TARGET)/release/tungsten check src/compiler/main.tg --check-tyvar-escape --max-errors=0'
