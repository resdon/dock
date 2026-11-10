#!/usr/bin/env bash
# WAYDOCK — Automated Quality Control Script
#
# Runs the complete static analysis and code quality pipeline.
# Execute from the project root: ./scripts/check.sh
#
# Usage:
#   ./scripts/check.sh          # Run all checks
#   ./scripts/check.sh --fast   # Skip slow checks (audit, machete)
#   ./scripts/check.sh --ci     # CI mode: fail on first error, no color

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Colors for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Parse arguments
FAST_MODE=false
CI_MODE=false
for arg in "$@"; do
    case $arg in
        --fast) FAST_MODE=true ;;
        --ci) CI_MODE=true; export NO_COLOR=1 ;;
        *) echo "Unknown option: $arg"; exit 1 ;;
    esac
done

if [ "$CI_MODE" = true ]; then
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    NC=''
fi

# Counters
PASSED=0
FAILED=0
SKIPPED=0

# Helper functions
pass() {
    echo -e "${GREEN}✓${NC} $1"
    PASSED=$((PASSED + 1))
}

fail() {
    echo -e "${RED}✗${NC} $1"
    FAILED=$((FAILED + 1))
    if [ "$CI_MODE" = true ]; then
        echo -e "${RED}CI mode: aborting on first failure${NC}"
        exit 1
    fi
}

skip() {
    echo -e "${YELLOW}⊘${NC} $1 (skipped)"
    SKIPPED=$((SKIPPED + 1))
}

section() {
    echo -e "\n${BLUE}═══ $1 ═══${NC}"
}

# ============================================================
# Layer 1: Rust Static Analysis and Code Quality
# ============================================================

section "Layer 1: Rust Static Analysis"

# 1.1. Formatting
echo "Running cargo fmt --check..."
if cargo fmt --all -- --check 2>&1; then
    pass "cargo fmt --check"
else
    fail "cargo fmt --check"
fi

# 1.2. Compilation
echo "Running cargo check --workspace --all-targets..."
if cargo check --workspace --all-targets 2>&1; then
    pass "cargo check --workspace --all-targets"
else
    fail "cargo check --workspace --all-targets"
fi

# 1.3. Clippy
echo "Running cargo clippy --workspace --all-targets..."
if cargo clippy --workspace --all-targets -- -D warnings 2>&1; then
    pass "cargo clippy --workspace --all-targets"
else
    fail "cargo clippy --workspace --all-targets"
fi

# 1.4. Unit tests
echo "Running cargo test --workspace..."
if cargo test --workspace 2>&1; then
    pass "cargo test --workspace"
else
    fail "cargo test --workspace"
fi

# 1.5. Dependency security audit (slow)
if [ "$FAST_MODE" = true ]; then
    skip "cargo audit (fast mode)"
else
    echo "Running cargo audit..."
    if command -v cargo-audit &>/dev/null; then
        if cargo audit 2>&1; then
            pass "cargo audit"
        else
            fail "cargo audit"
        fi
    else
        skip "cargo audit (cargo-audit not installed)"
    fi
fi

# 1.6. Unused dependency check (slow)
if [ "$FAST_MODE" = true ]; then
    skip "cargo machete (fast mode)"
else
    echo "Running cargo machete..."
    if command -v cargo-machete &>/dev/null; then
        if cargo machete 2>&1; then
            pass "cargo machete"
        else
            fail "cargo machete"
        fi
    else
        skip "cargo machete (cargo-machete not installed)"
    fi
fi

# 1.7. Dependency version check
echo "Checking dependency versions..."
if cargo tree -d 2>&1 | grep -q "warning"; then
    fail "cargo tree -d (duplicate dependencies found)"
else
    pass "cargo tree -d (no duplicate dependencies)"
fi

# ============================================================
# Summary
# ============================================================

echo -e "\n${BLUE}═══════════════════════════════${NC}"
echo -e "${GREEN}Passed: $PASSED${NC}"
echo -e "${RED}Failed: $FAILED${NC}"
echo -e "${YELLOW}Skipped: $SKIPPED${NC}"
echo -e "${BLUE}═══════════════════════════════${NC}"

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Some checks failed.${NC}"
    exit 1
fi

if [ $SKIPPED -gt 0 ]; then
    echo -e "${YELLOW}Some checks were skipped. Install cargo-audit and cargo-machete for full coverage.${NC}"
fi

echo -e "${GREEN}All checks passed!${NC}"
exit 0
