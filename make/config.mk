# make/config.mk — Shared build configuration
#
# Variables shared across all .mk files. Included first by the root Makefile.

# Toolchain PATH: agent shells spawn with a minimal env (no cargo, no mise).
# Prepend the user toolchain dirs when they exist so every recipe resolves
# cargo/rustc/python3 (shims), plus mise itself (~/.local/bin, for the
# `mise exec`/`mise install` recipes) without per-command PATH exports.
# Shims first so the
# .mise.toml-pinned python 3.12 beats system 3.9; rust version is unaffected
# by ordering — every entry point is rustup-backed and rust-toolchain.toml
# pins the in-repo toolchain. No-op on CI (mise absent; cargo already on PATH).
# NOTE (GNU make 3.81): this export reaches recipe lines, NOT parse-time
# $(shell ...) calls — parse-time resolvers must not rely on it.
MISE_SHIMS := $(HOME)/.local/share/mise/shims
CARGO_HOME_BIN := $(HOME)/.cargo/bin
LOCAL_BIN := $(HOME)/.local/bin
export PATH := $(if $(wildcard $(MISE_SHIMS)),$(MISE_SHIMS):)$(if $(wildcard $(CARGO_HOME_BIN)),$(CARGO_HOME_BIN):)$(if $(wildcard $(LOCAL_BIN)),$(LOCAL_BIN):)$(PATH)

# GNU make 3.81 fast-path gotcha: the PATH export above reaches recipe
# sub-shells and every child process, but NOT make's own executable lookup —
# a metachar-free recipe line ("cargo test -p foo") is exec'd directly using
# make's ORIGINAL environ PATH and fails in minimal-env (agent) shells.
# Recipe lines whose FIRST WORD is cargo/python3/mise must therefore use
# these variables. They resolve to the mise shim (absolute) when present —
# which also pins python3 to the .mise.toml version over system 3.9 — and
# stay bare otherwise (CI, activated dev shells). Lines with a shell
# metachar, an env-assignment prefix (VAR=x cargo ...), or a wrapper head
# (nice/env/sh) are unaffected either way.
CARGO := $(if $(wildcard $(MISE_SHIMS)/cargo),$(MISE_SHIMS)/cargo,cargo)
PYTHON3 := $(if $(wildcard $(MISE_SHIMS)/python3),$(MISE_SHIMS)/python3,python3)
MISE := $(if $(wildcard $(LOCAL_BIN)/mise),$(LOCAL_BIN)/mise,mise)

# LLVM 18 paths. Honor the caller's llvm-sys prefix (CI and Linux); fall back
# to Homebrew for local macOS builds.
LLVM_PREFIX := $(if $(LLVM_SYS_180_PREFIX),$(LLVM_SYS_180_PREFIX),$(shell brew --prefix llvm@18 2>/dev/null || echo "/opt/homebrew/opt/llvm@18"))
LLC := $(LLVM_PREFIX)/bin/llc
OPT := $(LLVM_PREFIX)/bin/opt
export LLVM_SYS_180_PREFIX := $(LLVM_PREFIX)

# The .tg gate binary (ADR 5.8.26d P1).
#
# `make tg-test` runs one process per file and each file elaborates the whole
# self-hosted compiler, so the profile the binary is BUILT with dominates the
# gate: measured 2026-08-05 at 25.1 min under target/debug against 10.5 min
# here, for a change with no semantic effect.
#
# It builds into an ISOLATED --target-dir rather than target/release because
# `target/release/tungsten` is ALREADY contested between --no-default-features
# consumers (make profile, make bench) and default-feature/codegen ones
# (self-compile, check-type-health, check-phase-invariants,
# doctor-self-test-full). A fifth consumer on a sixth feature-set combination
# would ping-pong a full release rebuild in both directions — the host-side
# feature-clobbering hazard CLAUDE.md documents for target/debug — and would
# spend the win this buys. Isolation costs 214 MB and a 20.8 s cold build
# (0.2 s warm); it breaks even inside the first run.
TG_GATE_TARGET_DIR ?= target/tg-gate
TG_GATE_BIN := $(TG_GATE_TARGET_DIR)/release/tungsten
TG_GATE_BUILD := $(CARGO) build -q --release --target-dir $(TG_GATE_TARGET_DIR) \
	-p tungsten_bootstrap --no-default-features

# Self-hosted compiler source
COMPILER_MAIN := src/compiler/main.tg

# Default max errors for self-hosted checks
MAX_ERRORS := 0

# ── Workspace-member submodule guard (ADR 14.8.26f D3) ──────────────────────
#
# The three vendored tools below are git SUBMODULES that are also `[workspace]`
# members. On a clone made without `--recurse-submodules` their
# directories are empty, and cargo then refuses to LOAD the workspace — before
# any recipe's binary is built or run:
#
#     error: failed to load manifest for workspace member `…/tools/scs-notify`
#
# `default-members` does not scope that away: it selects which members a bare
# cargo command BUILDS, not which manifests the workspace LOADS, and loading
# precedes selection.
#
# The failure is one layer below every gate, so a per-target prerequisite is the
# wrong shape — `test`, `build`, `lint`, `check-health` and every `cargo run -p`
# diagnostic hit it, and a guard replicated across that many targets would drift
# (D3). This is imposed at PARSE time in the file the root Makefile includes
# FIRST, which is the one place in the make surface that fronts every entry
# point without a single target opting in.
#
# `help` and the per-family `help-*` sub-makes it recurses into stay exempt, so
# a fresh clone can still discover what to run. That exemption is the only one,
# and `code-health`'s `workspace-submodule-guard` asserts both halves: the list
# below against `.gitmodules` ∩ `[workspace] members`, failing in either
# direction, and this file's position as the Makefile's first include.
#
# CEILING (D5): a bare `cargo` invocation is not a make recipe and is never
# fronted by this. Raw cargo keeps the manifest-path error. Accepted in writing
# by 14.8.26f § 2.2 rather than fixed — fixing it means dropping the members,
# which the D2 measurement vetoed.
WORKSPACE_SUBMODULES := tools/claude-hooks tools/scs-notify tools/code-health
# Only a checkout that DECLARES submodules can have uninitialised ones. The
# public repository ships neither `.gitmodules` nor these tools/ crates (its
# Cargo.toml has them stripped from `members`), so there the guard stands down
# rather than refusing every target (ADR 25.9.26l D5).
UNINITIALISED_SUBMODULES := $(if $(wildcard .gitmodules),$(strip $(foreach d,$(WORKSPACE_SUBMODULES),\
  $(if $(wildcard $(d)/Cargo.toml),,$(d)))))
# No goal on the command line means the default goal, which is never `help` here
# (make/core.mk's `build` is the first target defined) — so the placeholder must
# not match the exemption.
WORKSPACE_GUARD_GOALS := $(if $(MAKECMDGOALS),$(MAKECMDGOALS),default-goal)
# `$(info)` is reached through `$(foreach)`, and make strips a function
# argument's LEADING whitespace — so the indent must survive as a variable.
# The empty-variable sandwich is the standard make idiom for a literal space.
EMPTY :=
GUARD_INDENT := $(EMPTY)  $(EMPTY)
ifneq ($(UNINITIALISED_SUBMODULES),)
ifneq ($(strip $(filter-out help help-%,$(WORKSPACE_GUARD_GOALS))),)
$(info ERROR: uninitialised git submodule(s) — the cargo workspace cannot load.)
$(foreach d,$(UNINITIALISED_SUBMODULES),$(info $(GUARD_INDENT)Run: git submodule update --init $(d)))
$(error every workspace-member submodule must be initialised first)
endif
endif
