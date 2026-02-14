#!/bin/bash
# Golden test runner for Tungsten compiler
#
# Compares output from bootstrap driver against expected golden files.
# Strips ANSI color codes before comparison.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# Use relative path from project root for consistent module path behavior
GOLDEN_DIR="tests/golden"

# Change to project root for consistent relative paths
cd "$PROJECT_ROOT"

# Default compiler (bootstrap)
TUNGSTEN="./target/release/tungsten"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Strip ANSI color codes from output
strip_colors() {
    sed 's/\x1b\[[0-9;]*m//g'
}

# Normalize paths in output (convert absolute to relative)
normalize_paths() {
    sed "s|${PROJECT_ROOT}/||g"
}

# Run a golden test
run_test() {
    local cmd=$1      # check, run, or compile
    local file=$2     # .tg file
    local expected=$3 # .expected file
    
    # Run the command and capture output
    local actual
    if actual=$("$TUNGSTEN" "$cmd" "$file" 2>&1); then
        local exit_code=0
    else
        local exit_code=$?
    fi
    
    # Normalize output
    actual=$(echo "$actual" | strip_colors | normalize_paths)
    
    # Compare with expected
    if [ -f "$expected" ]; then
        local expected_content
        expected_content=$(cat "$expected")
        
        if [ "$actual" = "$expected_content" ]; then
            echo -e "${GREEN}✓${NC} $file"
            return 0
        else
            echo -e "${RED}✗${NC} $file"
            echo "  Expected:"
            echo "$expected_content" | head -5 | sed 's/^/    /'
            echo "  Actual:"
            echo "$actual" | head -5 | sed 's/^/    /'
            return 1
        fi
    else
        echo -e "${YELLOW}?${NC} $file (no expected file)"
        return 0
    fi
}

# Run a compile golden test: compile the file, run the binary, compare output
run_compile_test() {
    local file=$1     # .tg file
    local expected=$2 # .expected file
    
    local base="${file%.tg}"
    local binary="$base"
    
    # Compile the file
    if ! "$TUNGSTEN" compile "$file" 2>&1 | strip_colors > /dev/null; then
        echo -e "${RED}✗${NC} $file (compile failed)"
        return 1
    fi
    
    # Check binary was produced
    if [ ! -f "$binary" ]; then
        echo -e "${RED}✗${NC} $file (no binary produced)"
        return 1
    fi
    
    # Run the binary and capture output
    local actual
    if actual=$("$binary" 2>&1); then
        local exit_code=0
    else
        local exit_code=$?
    fi
    
    # Clean up the binary
    rm -f "$binary"
    
    # Compare with expected
    if [ -f "$expected" ]; then
        local expected_content
        expected_content=$(cat "$expected")
        
        if [ "$actual" = "$expected_content" ]; then
            echo -e "${GREEN}✓${NC} $file"
            return 0
        else
            echo -e "${RED}✗${NC} $file"
            echo "  Expected:"
            echo "$expected_content" | head -5 | sed 's/^/    /'
            echo "  Actual:"
            echo "$actual" | head -5 | sed 's/^/    /'
            return 1
        fi
    else
        echo -e "${YELLOW}?${NC} $file (no expected file)"
        rm -f "$binary"
        return 0
    fi
}

# Update expected file for compile tests: compile, run binary, save output
update_compile_expected() {
    local file=$1
    local expected=$2
    
    local base="${file%.tg}"
    local binary="$base"
    
    if ! "$TUNGSTEN" compile "$file" 2>&1 > /dev/null; then
        echo -e "${RED}Failed to compile${NC} $file"
        return 1
    fi
    
    if [ ! -f "$binary" ]; then
        echo -e "${RED}No binary produced${NC} for $file"
        return 1
    fi
    
    local actual
    actual=$("$binary" 2>&1 || true)
    rm -f "$binary"
    
    echo "$actual" > "$expected"
    echo -e "${GREEN}Updated${NC} $expected"
}

# Update expected files from bootstrap output
update_expected() {
    local cmd=$1
    local file=$2
    local expected=$3
    
    local actual
    actual=$("$TUNGSTEN" "$cmd" "$file" 2>&1 || true)
    actual=$(echo "$actual" | strip_colors | normalize_paths)
    
    echo "$actual" > "$expected"
    echo -e "${GREEN}Updated${NC} $expected"
}

# Run tests in a directory
run_category() {
    local category=$1
    local cmd=$2
    local dir="$GOLDEN_DIR/$category"
    
    if [ ! -d "$dir" ]; then
        echo "Creating $dir..."
        mkdir -p "$dir"
        return 0
    fi
    
    local passed=0
    local failed=0
    
    # Run tests for single .tg files in the category directory
    for tg_file in "$dir"/*.tg; do
        [ -f "$tg_file" ] || continue
        
        local base="${tg_file%.tg}"
        local expected="${base}.expected"
        
        if [ "$category" = "compile" ]; then
            if [ "$UPDATE_MODE" = "1" ]; then
                update_compile_expected "$tg_file" "$expected"
            else
                if run_compile_test "$tg_file" "$expected"; then
                    ((passed++)) || true
                else
                    ((failed++)) || true
                fi
            fi
        elif [ "$UPDATE_MODE" = "1" ]; then
            update_expected "$cmd" "$tg_file" "$expected"
        else
            if run_test "$cmd" "$tg_file" "$expected"; then
                ((passed++)) || true
            else
                ((failed++)) || true
            fi
        fi
    done
    
    # Run tests for subdirectories with main.tg (multi-file tests)
    for subdir in "$dir"/*/; do
        [ -d "$subdir" ] || continue
        # Skip hidden directories like .tungsten
        [[ "$(basename "$subdir")" == .* ]] && continue
        local main_file="${subdir}main.tg"
        [ -f "$main_file" ] || continue
        
        local expected="${subdir}main.expected"
        
        if [ "$category" = "compile" ]; then
            if [ "$UPDATE_MODE" = "1" ]; then
                update_compile_expected "$main_file" "$expected"
            else
                if run_compile_test "$main_file" "$expected"; then
                    ((passed++)) || true
                else
                    ((failed++)) || true
                fi
            fi
        elif [ "$UPDATE_MODE" = "1" ]; then
            update_expected "$cmd" "$main_file" "$expected"
        else
            if run_test "$cmd" "$main_file" "$expected"; then
                ((passed++)) || true
            else
                ((failed++)) || true
            fi
        fi
    done
    
    if [ "$UPDATE_MODE" != "1" ] && [ $((passed + failed)) -gt 0 ]; then
        echo ""
        echo "  $passed passed, $failed failed"
    fi
}

# Main
main() {
    local categories=("check" "run" "error" "compile")
    local specific_category=""
    UPDATE_MODE=0
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --update)
                UPDATE_MODE=1
                shift
                ;;
            check|run|error|compile)
                specific_category=$1
                shift
                ;;
            *)
                echo "Usage: $0 [--update] [check|run|error|compile]"
                exit 1
                ;;
        esac
    done
    
    # Check if compiler exists
    if [ ! -f "$TUNGSTEN" ]; then
        echo "Building compiler..."
        (cd "$PROJECT_ROOT" && cargo build --release -p tungsten_bootstrap --no-default-features)
    fi
    
    echo "Golden Test Runner"
    echo "=================="
    echo ""
    
    if [ -n "$specific_category" ]; then
        categories=("$specific_category")
    fi
    
    for category in "${categories[@]}"; do
        echo "[$category]"
        case $category in
            check)   run_category "$category" "check" ;;
            run)     run_category "$category" "run" ;;
            error)   run_category "$category" "check" ;;  # error tests use check command
            compile) run_category "$category" "compile" ;;
        esac
        echo ""
    done
}

main "$@"
