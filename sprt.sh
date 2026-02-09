#!/bin/bash
set -euo pipefail

# SPRT testing script for itgo
# Usage: ./sprt.sh --test-dir DIR --result-dir DIR [OPTIONS] [-- ARENA_ARGS...]
#
# Required:
#   -t, --test-dir DIR   Directory for engine binaries (must exist)
#   -r, --result-dir DIR Directory for result files (created if needed)
#
# Modes:
#   (default)            Test working directory vs HEAD
#   -d, --dev COMMIT     Test specific commit as dev (enables commit mode)
#   -b, --base COMMIT    Base commit to test against (default: HEAD)
#
# Other:
#   -h, --help           Show this help message
#
# Everything after '--' is passed directly to myu-arena.
#
# Examples:
#   ./sprt.sh -t .test -r .result -- --elo1 5
#   ./sprt.sh -t .test -r .result -b HEAD~5
#   ./sprt.sh -t .test -r .result -d HEAD -b HEAD~1

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_DIR=""
RESULT_DIR=""

DEV_COMMIT=""
BASE_COMMIT="HEAD"
ARENA_ARGS=()

show_help() {
    sed -n '3,22p' "$0" | sed 's/^# \?//'
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -t|--test-dir)
            TEST_DIR="$2"
            shift 2
            ;;
        -r|--result-dir)
            RESULT_DIR="$2"
            shift 2
            ;;
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

# Validate required arguments
if [[ -z "$TEST_DIR" ]]; then
    echo "Error: --test-dir is required" >&2
    echo "Use --help for usage information" >&2
    exit 1
fi

if [[ -z "$RESULT_DIR" ]]; then
    echo "Error: --result-dir is required" >&2
    echo "Use --help for usage information" >&2
    exit 1
fi

if [[ ! -d "$TEST_DIR" ]]; then
    echo "Error: test directory does not exist: $TEST_DIR" >&2
    exit 1
fi

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

# Prepare output directory
mkdir -p "$RESULT_DIR"
GAMES_FILE="$RESULT_DIR/games_$(date +%s).txt"

echo ""
echo "=== Running SPRT ==="
echo ""

# Run arena with user-provided args
exec cargo run --release --manifest-path "$SCRIPT_DIR/Cargo.toml" -p myu-arena -- test \
    --dev "$TEST_DIR/dev" \
    --base "$TEST_DIR/base" \
    --logs-dir "$RESULT_DIR" \
    --games-file "$GAMES_FILE" \
    "${ARENA_ARGS[@]}"
