#!/usr/bin/env bash
# Usage: scripts/tui-screenshot.sh <output-png> [-- gitish-args...]
#
# Captures a PNG screenshot of gitish by running it inside a detached tmux
# pane, capturing the live alternate-screen content with tmux capture-pane,
# and rendering it to a styled image via termshot --raw-read.
#
# By default runs gitish in a fresh empty git repo (no staged/unstaged changes).
# Set GITISH_REPO to an existing path to screenshot a specific repo state.
#
# Requires: tmux (in PATH), termshot (nix run nixpkgs#termshot if not in PATH).
# The release binary is built automatically if missing.
set -euo pipefail

OUTPUT_PNG="${1:?Usage: scripts/tui-screenshot.sh <output-png> [-- gitish-args...]}"
shift; [[ "${1:-}" == "--" ]] && shift || true

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GITISH_BIN="$PROJECT_ROOT/target/release/gitish"

if [[ ! -x "$GITISH_BIN" ]]; then
    echo "Building gitish release binary..." >&2
    (cd "$PROJECT_ROOT" && nix develop --command cargo build --release 2>&1)
fi

# Set up a clean temp repo unless the caller provided one.
CLEANUP_REPO=0
if [[ -z "${GITISH_REPO:-}" ]]; then
    GITISH_REPO="$(mktemp -d)"
    CLEANUP_REPO=1
    git -C "$GITISH_REPO" init -q
    git -C "$GITISH_REPO" config user.email test@test.com
    git -C "$GITISH_REPO" config user.name Test
fi

COLS=120
ROWS=36
SESSION="gitish-ss-$$"

# Start gitish in a detached tmux pane.
tmux new-session -d -s "$SESSION" -x "$COLS" -y "$ROWS" \
    "cd '$GITISH_REPO' && '$GITISH_BIN' $*"

# Give the TUI time to render its first frame.
sleep 2

# Capture the live pane content (alternate screen) with ANSI escape codes.
RAW="$(mktemp --suffix=.ans)"
trap 'rm -f "$RAW"; [[ "$CLEANUP_REPO" == 1 ]] && rm -rf "$GITISH_REPO"; tmux kill-session -t "$SESSION" 2>/dev/null || true' EXIT
tmux capture-pane -t "$SESSION" -p -e > "$RAW"

tmux kill-session -t "$SESSION" 2>/dev/null || true

# Render the capture to a PNG.
mkdir -p "$(dirname "$OUTPUT_PNG")"

if ! command -v termshot &>/dev/null; then
    TERMSHOT_CMD="nix run nixpkgs#termshot --"
else
    TERMSHOT_CMD="termshot"
fi

# --raw-read reads the ANSI stream; -C locks the column width so the frame
# renders at the same width as the tmux pane.
$TERMSHOT_CMD --raw-read "$RAW" -C "$COLS" -f "$OUTPUT_PNG"

echo "$OUTPUT_PNG"
