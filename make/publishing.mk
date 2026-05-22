# make/publishing.mk — Snapshot publishing targets
#
# Commands for building and publishing Tungsten snapshots.

STAGING_DIR ?= ../tungsten-staging
VERSION := $(shell cat VERSION 2>/dev/null || echo "0.0.0")

.PHONY: snapshot-build snapshot check-publish-drift sync-publish bump-version check-version

# Build the snapshot tool
snapshot-build:
	cargo build --release -p tungsten_snapshot

# Run the snapshot tool (pass arguments after --)
# Usage: make snapshot -- dry-run v1.0.0
#        make snapshot -- publish v1.0.0
snapshot: snapshot-build
	./target/release/tungsten-snapshot $(filter-out $@,$(MAKECMDGOALS))

# Check if publish/ has drifted between tungsten-private and tungsten-staging
check-publish-drift:
	@echo "Checking publish tool drift (private vs staging)..."
	@diff -rq publish/src/ $(STAGING_DIR)/publish/src/ > /dev/null 2>&1 \
		&& diff -q publish/build.rs $(STAGING_DIR)/publish/build.rs > /dev/null 2>&1 \
		&& diff -q publish/Cargo.toml $(STAGING_DIR)/publish/Cargo.toml > /dev/null 2>&1 \
		&& echo "  No drift detected." \
		|| (echo "  DRIFT DETECTED. Files differ:"; \
		    diff -rq publish/src/ $(STAGING_DIR)/publish/src/ 2>/dev/null; \
		    diff -q publish/build.rs $(STAGING_DIR)/publish/build.rs 2>/dev/null; \
		    diff -q publish/Cargo.toml $(STAGING_DIR)/publish/Cargo.toml 2>/dev/null; \
		    echo "  Run 'make sync-publish' to copy private → staging."; \
		    exit 1)

# Copy publish/ from tungsten-private to tungsten-staging
sync-publish:
	@echo "Syncing publish tool to $(STAGING_DIR)..."
	@rm -rf $(STAGING_DIR)/publish/src
	@cp -R publish/src $(STAGING_DIR)/publish/src
	@cp publish/build.rs $(STAGING_DIR)/publish/build.rs
	@cp publish/Cargo.toml $(STAGING_DIR)/publish/Cargo.toml
	@echo "  Done. Commit in staging separately."

# Bump version across all source-of-truth locations.
# Usage: make bump-version V=1.6.0
bump-version:
ifndef V
	$(error Usage: make bump-version V=<version>, e.g. make bump-version V=1.6.0)
endif
	@echo "Bumping version to $(V)..."
	@echo "$(V)" > VERSION
	@# Cargo.toml workspace version
	@sed -i '' 's/^version = ".*"/version = "$(V)"/' Cargo.toml
	@# L2 self-hosted version strings
	@sed -i '' 's/tungsten [0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]* (self-hosted)/tungsten $(V) (self-hosted)/' \
		src/compiler/driver/cli/output.tg \
		src/compiler/driver/pipeline/info.tg
	@# Pre-compiled .ll IR (string length changes require manual review if version length differs)
	@find src/compiler/target/ll -name '*.ll' -exec grep -l 'self-hosted' {} \; | \
		xargs -I{} sed -i '' 's/tungsten [0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]* (self-hosted)/tungsten $(V) (self-hosted)/' {}
	@echo "  Updated: VERSION, Cargo.toml, output.tg, info.tg, .ll files"
	@echo "  Run 'make check-version' to verify consistency."

# Verify all version locations are consistent with VERSION file.
check-version:
	@echo "Checking version consistency (expected: $(VERSION))..."
	@EXIT=0; \
	grep -q 'version = "$(VERSION)"' Cargo.toml || { echo "  MISMATCH: Cargo.toml"; EXIT=1; }; \
	grep -q 'tungsten $(VERSION) (self-hosted)' src/compiler/driver/cli/output.tg || { echo "  MISMATCH: output.tg"; EXIT=1; }; \
	grep -q 'tungsten $(VERSION) (self-hosted)' src/compiler/driver/pipeline/info.tg || { echo "  MISMATCH: info.tg"; EXIT=1; }; \
	if [ $$EXIT -eq 0 ]; then echo "  All version locations consistent."; else exit 1; fi

# Swallow any extra arguments passed after -- so Make doesn't treat them as targets
%:
	@:

# Help section for publishing
.PHONY: help-publishing
help-publishing:
	@echo ""
	@echo "Publishing:"
	@echo "  make snapshot-build            - Build the snapshot tool"
	@echo "  make snapshot -- dry-run <ver> - Preview a snapshot"
	@echo "  make snapshot -- publish <ver> - Publish a snapshot"
	@echo "  make check-publish-drift       - Check for drift between private and staging publish/"
	@echo "  make sync-publish              - Copy publish/ from private to staging"
	@echo "  make bump-version V=<ver>      - Update version across all source files"
	@echo "  make check-version             - Verify version consistency"
