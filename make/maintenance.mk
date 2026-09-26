# make/maintenance.mk
# Housekeeping targets: reclaiming disk used by build artifacts and caches.
#
# These live in their own file rather than in quality.mk or devcontainer.mk
# because both of those are at the `mk-size` ceilings their allowlist entries
# record, and growing a baselined file works against ADR 31.7.26c (the makefile
# split that owns paying those entries down).

.PHONY: cache-clean cache-clean-dry-run cache-status devcontainer-image-prune

## Remove every `.tungsten` elaboration cache under the working tree.
##
## Use this instead of `rm -rf` on cache directories. `tungsten cache clean`
## knows where caches live — including the ones written NEXT TO a test entry
## file rather than at the repo root, which a hand-written `rm -rf .tungsten`
## misses — and it skips `target/`. A stray `rm -rf` both misses those and can
## delete something else if the path is wrong.
##
## Needed after: changing type definitions or the elaborator, and after adding,
## removing or reordering `Expr`/`Item`/`Stmt` AST variants (bincode encodes
## positionally, so a stale cached AST deserializes as a "capacity overflow"
## panic rather than a clean miss).
cache-clean:
	@$(CARGO) build -q -p tungsten_bootstrap --no-default-features
	@target/debug/tungsten cache clean

## Preview what `cache-clean` would remove, without removing it.
cache-clean-dry-run:
	@$(CARGO) build -q -p tungsten_bootstrap --no-default-features
	@target/debug/tungsten cache clean --dry-run

## Report the current project's elaboration-cache state.
cache-status:
	@$(CARGO) build -q -p tungsten_bootstrap --no-default-features
	@target/debug/tungsten cache status

## Reclaim disk used by Tungsten's OWN Docker images.
##
## Scope is deliberately narrow: images whose repository starts with `tungsten`
## or `vsc-tungsten` — this repo's devcontainer builds and `make snapshot`
## artifacts. Other projects' images, base images, and every named volume are
## untouched. `docker rmi` itself refuses to remove an image an existing
## container references, so the live devcontainer survives; those refusals are
## reported, not treated as failures.
##
## This does NOT touch the `tungsten-arm-target` build volume — that is a warm
## incremental cache worth keeping, and `make devcontainer-target-reset` is the
## deliberate way to wipe it.
##
## Preview first with: make devcontainer-image-prune DRY_RUN=1
devcontainer-image-prune:
	@set -e; \
	imgs=$$(docker images --format '{{.ID}}\t{{.Repository}}:{{.Tag}}\t{{.Size}}' \
	        | awk -F'\t' '$$2 ~ /^(vsc-)?tungsten/ {print}'); \
	if [ -z "$$imgs" ]; then echo "[image-prune] no Tungsten images found"; exit 0; fi; \
	echo "[image-prune] candidate images (in-use ones will be refused by docker):"; \
	echo "$$imgs" | awk -F'\t' '{printf "  %-10s %-72s %s\n", $$1, $$2, $$3}'; \
	if [ -n "$(DRY_RUN)" ]; then echo "[image-prune] DRY_RUN set — nothing removed"; exit 0; fi; \
	echo "$$imgs" | cut -f1 | sort -u | xargs docker rmi 2>&1 \
	  | sed -e 's/^/  /' || true; \
	echo "[image-prune] ✓ done — remaining Tungsten images:"; \
	docker images --format '  {{.Repository}}:{{.Tag}}\t{{.Size}}' \
	  | awk -F'\t' '$$1 ~ /(vsc-)?tungsten/ {print}' || true

HELP_SECTIONS += maintenance

.PHONY: help-maintenance
help-maintenance:
	@echo ""
	@echo "Maintenance:"
	@echo "  make cache-clean                - Remove all .tungsten elaboration caches (never rm -rf)"
	@echo "  make cache-clean-dry-run        - Preview what cache-clean would remove"
	@echo "  make cache-status               - Show this project's elaboration-cache state"
	@echo "  make devcontainer-image-prune   - Reclaim disk from Tungsten's own Docker images"
	@echo "                                    (DRY_RUN=1 to preview; other projects untouched)"
