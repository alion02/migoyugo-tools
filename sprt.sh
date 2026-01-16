#!/bin/bash
set -euo pipefail

# SPRT testing script for itgo
# Usage: ./sprt.sh [OPTIONS] [-- ARENA_ARGS...]
#
# Modes:
#   (default)            Test working directory vs HEAD
#   -d, --dev COMMIT     Test specific commit as dev (enables commit mode)
#   -b, --base COMMIT    Base commit to test against (default: HEAD)
#   -h, --help           Show this help message
#
# Everything after '--' is passed directly to myu-arena.
#
# Examples:
#   ./sprt.sh                              # working dir vs HEAD
#   ./sprt.sh -- --elo1 5                  # working dir vs HEAD, custom elo1
#   ./sprt.sh -b HEAD~5                    # working dir vs HEAD~5
#   ./sprt.sh -d HEAD -b HEAD~1            # commit vs commit mode

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_DIR="$SCRIPT_DIR/.test"
RESULT_DIR="$SCRIPT_DIR/.result"

DEV_COMMIT=""
BASE_COMMIT="HEAD"
ARENA_ARGS=()

show_help() {
    sed -n '3,16p' "$0" | sed 's/^# \?//'
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -d|--dev)
            DEV_COMMIT="$2"
            shift 2
            ;;
        -b|--base)
            BASE_COMMIT="$2"
            shift 2
            ;;
        -h|--help)
            show_help
            ;;
        --)
            shift
            ARENA_ARGS=("$@")
            break
            ;;
        *)
            echo "Unknown option: $1" >&2
            echo "Use --help for usage information" >&2
            exit 1
            ;;
    esac
done

# Determine mode
if [[ -z "$DEV_COMMIT" ]]; then
    # Working directory mode (default)
    WORKING_DIR_MODE=true
    BASE_HASH=$(git -C "$SCRIPT_DIR" rev-parse --short "$BASE_COMMIT")
    echo "=== SPRT Test: working dir (dev) vs $BASE_HASH (base) ==="
else
    # Commit vs commit mode
    WORKING_DIR_MODE=false
    DEV_HASH=$(git -C "$SCRIPT_DIR" rev-parse --short "$DEV_COMMIT")
    BASE_HASH=$(git -C "$SCRIPT_DIR" rev-parse --short "$BASE_COMMIT")
    echo "=== SPRT Test: $DEV_HASH (dev) vs $BASE_HASH (base) ==="
fi
echo ""

# Save current state
ORIGINAL_REF=$(git -C "$SCRIPT_DIR" symbolic-ref --short HEAD 2>/dev/null || git -C "$SCRIPT_DIR" rev-parse HEAD)
STASH_CREATED=false

cleanup() {
    echo ""
    echo "Restoring original state ($ORIGINAL_REF)..."
    git -C "$SCRIPT_DIR" checkout --quiet "$ORIGINAL_REF"
    if $STASH_CREATED; then
        echo "Restoring stashed changes..."
        git -C "$SCRIPT_DIR" stash pop --quiet
    fi
}

if $WORKING_DIR_MODE; then
    # Build dev from working directory first (before any git operations)
    echo "Building dev (working dir)..."
    cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml" -p itgo --quiet
    cp "$SCRIPT_DIR/target/release/itgo" "$TEST_DIR/dev"
    chmod +x "$TEST_DIR/dev"
    echo "  -> $TEST_DIR/dev"

    # Stash changes to checkout base
    if ! git -C "$SCRIPT_DIR" diff --quiet || ! git -C "$SCRIPT_DIR" diff --cached --quiet; then
        echo "Stashing uncommitted changes..."
        git -C "$SCRIPT_DIR" stash push -m "sprt.sh auto-stash"
        STASH_CREATED=true
    fi
    trap cleanup EXIT

    # Build base version
    echo "Building base ($BASE_HASH)..."
    git -C "$SCRIPT_DIR" checkout --quiet "$BASE_COMMIT"
    cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml" -p itgo --quiet
    cp "$SCRIPT_DIR/target/release/itgo" "$TEST_DIR/base"
    chmod +x "$TEST_DIR/base"
    echo "  -> $TEST_DIR/base"
else
    # Commit mode: stash first, then build both commits
    if ! git -C "$SCRIPT_DIR" diff --quiet || ! git -C "$SCRIPT_DIR" diff --cached --quiet; then
        echo "Stashing uncommitted changes..."
        git -C "$SCRIPT_DIR" stash push -m "sprt.sh auto-stash"
        STASH_CREATED=true
    fi
    trap cleanup EXIT

    # Build dev version
    echo "Building dev ($DEV_HASH)..."
    git -C "$SCRIPT_DIR" checkout --quiet "$DEV_COMMIT"
    cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml" -p itgo --quiet
    cp "$SCRIPT_DIR/target/release/itgo" "$TEST_DIR/dev"
    chmod +x "$TEST_DIR/dev"
    echo "  -> $TEST_DIR/dev"

    # Build base version
    echo "Building base ($BASE_HASH)..."
    git -C "$SCRIPT_DIR" checkout --quiet "$BASE_COMMIT"
    cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml" -p itgo --quiet
    cp "$SCRIPT_DIR/target/release/itgo" "$TEST_DIR/base"
    chmod +x "$TEST_DIR/base"
    echo "  -> $TEST_DIR/base"
fi

# Restore original state before running arena
echo "Restoring original state for arena..."
git -C "$SCRIPT_DIR" checkout --quiet "$ORIGINAL_REF"
if $STASH_CREATED; then
    git -C "$SCRIPT_DIR" stash pop --quiet
    STASH_CREATED=false
fi
trap - EXIT

# Prepare output file
mkdir -p "$RESULT_DIR"
GAMES_FILE="$RESULT_DIR/games_$(date +%s).txt"

echo ""
echo "=== Running SPRT ==="
echo ""

# Default arena arguments
DEFAULT_ARGS=(
    --dev "$TEST_DIR/dev"
    --base "$TEST_DIR/base"
    --max-pairs 50000
    --concurrency 14
    --time-ms 100
    --opening-book "$TEST_DIR/book.txt"
    --games-file "$GAMES_FILE"
    --logs-dir "$RESULT_DIR"
    --elo1 10
)

# Run arena with defaults, allowing user args to override
exec cargo run --release --manifest-path "$SCRIPT_DIR/Cargo.toml" -p myu-arena -- test "${DEFAULT_ARGS[@]}" "${ARENA_ARGS[@]}"
