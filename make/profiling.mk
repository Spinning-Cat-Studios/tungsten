# make/profiling.mk — Profiling and benchmarking targets
#
# CPU profiling via samply and statistical benchmarking via hyperfine.
# Requires: mise install (installs samply + hyperfine from mise.toml)
#
# The canonical workload is `check src/compiler/main.tg` — the largest input
# available, exercising the full lexer → parser → elaborator pipeline.

.PHONY: setup-profiling profile bench bench-compare profile-memory profile-heaptrack profile-memory-compiled

# Install profiling/benchmarking tools via mise
setup-profiling:
	@command -v mise >/dev/null 2>&1 || { echo "Error: mise not found. Install from https://mise.jdx.dev/"; exit 1; }
	@mise which samply >/dev/null 2>&1 && mise which hyperfine >/dev/null 2>&1 || { \
		echo "Installing profiling tools via mise..."; \
		mise install; \
		echo ""; \
		echo "✓ Installed samply + hyperfine"; \
	}

# Profile the compiler (CPU sampling via samply, opens browser UI)
# Usage: make profile              — profiles check on self-hosted compiler
#        make profile FILE=<file>  — profiles check on a specific file
profile: setup-profiling
	@echo "Building release binary..."
	@cargo build --release -p tungsten_bootstrap --no-default-features
	@echo ""
ifndef FILE
	@echo "Profiling: tungsten check src/compiler/main.tg"
	mise exec -- samply record ./target/release/tungsten check src/compiler/main.tg
else
	@echo "Profiling: tungsten check $(FILE)"
	mise exec -- samply record ./target/release/tungsten check $(FILE)
endif

# Benchmark the compiler (statistical timing via hyperfine)
# Usage: make bench              — benchmarks check on self-hosted compiler
#        make bench FILE=<file>  — benchmarks check on a specific file
bench: setup-profiling
	@echo "Building release binary..."
	@cargo build --release -p tungsten_bootstrap --no-default-features
	@echo ""
ifndef FILE
	@echo "Benchmarking: tungsten check src/compiler/main.tg"
	mise exec -- hyperfine --warmup 2 --runs 10 './target/release/tungsten check src/compiler/main.tg'
else
	@echo "Benchmarking: tungsten check $(FILE)"
	mise exec -- hyperfine --warmup 2 --runs 10 './target/release/tungsten check $(FILE)'
endif

# Compare two compiler binaries (e.g., bootstrap vs self-hosted)
# Usage: make bench-compare A=./tungsten1 B=./tungsten2
bench-compare: setup-profiling
ifndef A
	@echo "Usage: make bench-compare A=<binary1> B=<binary2>"
	@echo "Example: make bench-compare A=./tungsten1 B=./tungsten2"
else ifndef B
	@echo "Usage: make bench-compare A=<binary1> B=<binary2>"
	@echo "Example: make bench-compare A=./tungsten1 B=./tungsten2"
else
	mise exec -- hyperfine --warmup 2 --runs 10 \
	  '$(A) check src/compiler/main.tg' \
	  '$(B) check src/compiler/main.tg'
endif

# Run the benchmark suite (requires devcontainer for .tg compilation)
.PHONY: devcontainer-benchmark benchmark
devcontainer-benchmark:
	devcontainer exec --workspace-folder . cargo run --release -p tungsten-bench -- run --bench-dir benchmarks

# Alias for ADR compatibility
benchmark: devcontainer-benchmark

# --- Memory Profiling (devcontainer) ---

# Memory profile the compiler via valgrind/massif (devcontainer)
# Usage: make profile-memory              — profiles check on self-hosted compiler
#        make profile-memory FILE=<file>  — profiles check on a specific file
.PHONY: profile-memory
profile-memory:
ifndef FILE
	@echo "Memory profiling: tungsten check src/compiler/main.tg (massif)"
	devcontainer exec --workspace-folder . bash -c '\
		cd /workspaces/tungsten-private && \
		valgrind --tool=massif --pages-as-heap=yes \
		  ./target/release/tungsten check src/compiler/main.tg 2>&1 && \
		ms_print massif.out.$$(ls -t massif.out.* | head -1 | sed "s/massif.out.//") | head -80'
else
	@echo "Memory profiling: tungsten check $(FILE) (massif)"
	devcontainer exec --workspace-folder . bash -c '\
		cd /workspaces/tungsten-private && \
		valgrind --tool=massif --pages-as-heap=yes \
		  ./target/release/tungsten check $(FILE) 2>&1 && \
		ms_print massif.out.$$(ls -t massif.out.* | head -1 | sed "s/massif.out.//") | head -80'
endif

# Memory profile the compiler via heaptrack (devcontainer)
# Produces a .gz file — view with heaptrack_print (container) or heaptrack_gui (host)
# Usage: make profile-heaptrack              — profiles check on self-hosted compiler
#        make profile-heaptrack FILE=<file>  — profiles check on a specific file
.PHONY: profile-heaptrack
profile-heaptrack:
ifndef FILE
	@echo "Memory profiling: tungsten check src/compiler/main.tg (heaptrack)"
	devcontainer exec --workspace-folder . bash -c '\
		cd /workspaces/tungsten-private && \
		heaptrack ./target/release/tungsten check src/compiler/main.tg && \
		echo "" && \
		echo "Heaptrack summary:" && \
		heaptrack_print heaptrack.tungsten.*.zst 2>/dev/null | tail -20 || \
		heaptrack_print heaptrack.tungsten.*.gz 2>/dev/null | tail -20 || \
		echo "Use heaptrack_print <file> to view results"'
else
	@echo "Memory profiling: tungsten check $(FILE) (heaptrack)"
	devcontainer exec --workspace-folder . bash -c '\
		cd /workspaces/tungsten-private && \
		heaptrack ./target/release/tungsten check $(FILE) && \
		echo "" && \
		echo "Heaptrack summary:" && \
		heaptrack_print heaptrack.tungsten.*.zst 2>/dev/null | tail -20 || \
		heaptrack_print heaptrack.tungsten.*.gz 2>/dev/null | tail -20 || \
		echo "Use heaptrack_print <file> to view results"'
endif

# Memory profile a compiled .tg binary via valgrind/massif (devcontainer)
# Usage: make profile-memory-compiled BIN=/tmp/bench_factorial
.PHONY: profile-memory-compiled
profile-memory-compiled:
ifndef BIN
	@echo "Usage: make profile-memory-compiled BIN=<binary>"
	@echo "Example: make profile-memory-compiled BIN=/tmp/bench_factorial"
else
	@echo "Memory profiling: $(BIN) (massif)"
	devcontainer exec --workspace-folder . bash -c '\
		cd /workspaces/tungsten-private && \
		valgrind --tool=massif $(BIN) 2>&1 && \
		ms_print massif.out.$$(ls -t massif.out.* | head -1 | sed "s/massif.out.//") | head -80'
endif

# Help section for profiling
.PHONY: help-profiling
help-profiling:
	@echo ""
	@echo "Profiling & Benchmarking (requires: mise install):"
	@echo "  make setup-profiling                   - Install samply + hyperfine via mise"
	@echo "  make profile                           - CPU profile check on self-hosted compiler (samply)"
	@echo "  make profile FILE=<file>               - CPU profile check on a specific file"
	@echo "  make bench                             - Benchmark check on self-hosted compiler"
	@echo "  make bench FILE=<file>                 - Benchmark check on a specific file"
	@echo "  make bench-compare A=./bin1 B=./bin2   - Compare two binaries"
	@echo "  make devcontainer-benchmark            - Run full benchmark suite (devcontainer)"
	@echo ""
	@echo "Memory Profiling (devcontainer):"
	@echo "  make profile-memory                    - Heap profile compiler (valgrind/massif)"
	@echo "  make profile-memory FILE=<file>        - Heap profile compiler on a specific file"
	@echo "  make profile-heaptrack                 - Heap profile compiler (heaptrack)"
	@echo "  make profile-heaptrack FILE=<file>     - Heap profile compiler on a specific file"
	@echo "  make profile-memory-compiled BIN=<bin> - Heap profile a compiled .tg binary (massif)"
