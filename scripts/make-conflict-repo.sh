#!/usr/bin/env bash
# Creates a temporary git repository in a state with a merge conflict,
# then prints the path to stdout so callers can pass it as GITISH_REPO.
#
# Usage:
#   GITISH_REPO="$(scripts/make-conflict-repo.sh)" \
#     nix develop --command scripts/tui-screenshot.sh docs/screenshots/issue-4.png
#
# The repository contains one committed file ("README.md") with a resolved
# history, plus one file ("feature.rs") in a conflicted state with two
# conflict blocks, ready for gitish to display the merge conflict UI.
set -euo pipefail

REPO="$(mktemp -d)"

git -C "$REPO" init -q
git -C "$REPO" config user.email test@test.com
git -C "$REPO" config user.name Test

# Commit a clean base on main
cat > "$REPO/README.md" <<'EOF'
# My Project
A simple project.
EOF
cat > "$REPO/feature.rs" <<'EOF'
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

fn farewell(name: &str) -> String {
    format!("Goodbye, {}!", name)
}
EOF

git -C "$REPO" add README.md feature.rs
git -C "$REPO" commit -qm "initial commit"

# Create a feature branch with changes
git -C "$REPO" checkout -qb feature

cat > "$REPO/feature.rs" <<'EOF'
fn greet(name: &str) -> String {
    format!("Hi there, {}! Welcome!", name)
}

fn farewell(name: &str) -> String {
    format!("See you later, {}!", name)
}
EOF

git -C "$REPO" add feature.rs
git -C "$REPO" commit -qm "friendlier messages"

# Go back to main and make a conflicting change
git -C "$REPO" checkout -q main

cat > "$REPO/feature.rs" <<'EOF'
fn greet(name: &str) -> String {
    format!("Greetings, {}.", name)
}

fn farewell(name: &str) -> String {
    format!("Farewell, {}.", name)
}
EOF

git -C "$REPO" add feature.rs
git -C "$REPO" commit -qm "formal messages"

# Trigger a merge conflict — exits non-zero with conflict, which is expected
git -C "$REPO" merge --no-commit feature >/dev/null 2>&1 || true

echo "$REPO"
