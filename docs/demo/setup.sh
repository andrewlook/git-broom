#!/usr/bin/env bash
#
# Creates a realistic demo repo for recording git-broom demos.
#
# Prerequisites:
#   - gh auth login (GitHub CLI authenticated)
#   - git-broom on $PATH (cargo install --path .)
#
# Usage:
#   ./setup.sh <github-owner/repo>
#
# Example:
#   gh repo create alook/broom-demo --public --confirm
#   ./setup.sh alook/broom-demo
#
# This creates branches in every git-broom category:
#   gone      — remote branch deleted, local tracking branch remains
#   unpushed  — local-only, never pushed
#   nopr      — pushed but no PR created
#   closed    — PR was opened then closed without merging
#   merged    — PR was merged (squash-merge, branch left behind)
#   pr        — PR is still open
#
# After setup, cd into the repo and record with VHS or asciinema.

set -euo pipefail

REPO="${1:?Usage: $0 <owner/repo>}"
REPO_NAME="${REPO##*/}"
DIR="/tmp/${REPO_NAME}"

echo "==> Setting up demo repo at $DIR"
rm -rf "$DIR"
mkdir -p "$DIR"
cd "$DIR"

git init
git checkout -b main

# Seed the repo with a few realistic files so commits look natural.
cat > README.md << 'EOF'
# my-app

A web application with user authentication and a REST API.
EOF

cat > server.py << 'PYEOF'
from flask import Flask

app = Flask(__name__)

@app.route("/health")
def health():
    return {"status": "ok"}

if __name__ == "__main__":
    app.run()
PYEOF

git add -A
git commit -m "initial commit"

git remote add origin "git@github.com:${REPO}.git"
git push -u origin main

# -------------------------------------------------------------------
# Helper: create a branch with a realistic commit, push it
# -------------------------------------------------------------------
make_branch() {
    local branch="$1"
    local file="$2"
    local content="$3"
    local msg="$4"

    git checkout -b "$branch" main
    echo "$content" > "$file"
    git add "$file"
    git commit -m "$msg"
    git push -u origin "$branch"
    git checkout main
}

# -------------------------------------------------------------------
# MERGED — branches with merged PRs (squash-merge, branch left behind)
# -------------------------------------------------------------------
echo "==> Creating merged PR branches..."

make_branch "feature/add-login-page" "login.html" \
    '<form action="/login"><input name="email"></form>' \
    "feat: add login page with email form"

PR_NUM=$(gh pr create --repo "$REPO" --head "feature/add-login-page" \
    --title "Add login page" --body "Basic login form" | grep -oE '[0-9]+$')
gh pr merge "$PR_NUM" --repo "$REPO" --squash --delete-branch=false
# Restore the local branch (gh merge doesn't touch local)
git fetch origin

make_branch "fix/api-timeout" "server.py" \
    'from flask import Flask
app = Flask(__name__)
app.config["TIMEOUT"] = 30

@app.route("/health")
def health():
    return {"status": "ok"}' \
    "fix: increase API timeout to 30s"

PR_NUM=$(gh pr create --repo "$REPO" --head "fix/api-timeout" \
    --title "Fix API timeout" --body "Bump timeout to 30s" | grep -oE '[0-9]+$')
gh pr merge "$PR_NUM" --repo "$REPO" --squash --delete-branch=false
git fetch origin

make_branch "chore/update-deps" "requirements.txt" \
    'flask==3.1.0
requests==2.32.0' \
    "chore: update flask and requests"

PR_NUM=$(gh pr create --repo "$REPO" --head "chore/update-deps" \
    --title "Update dependencies" --body "Routine dep bump" | grep -oE '[0-9]+$')
gh pr merge "$PR_NUM" --repo "$REPO" --squash --delete-branch=false
git fetch origin

# -------------------------------------------------------------------
# CLOSED — PRs opened then closed without merging
# -------------------------------------------------------------------
echo "==> Creating closed PR branches..."

make_branch "feature/dark-mode" "theme.css" \
    'body { background: #1a1a1a; color: #eee; }' \
    "feat: add dark mode styles"

PR_NUM=$(gh pr create --repo "$REPO" --head "feature/dark-mode" \
    --title "Add dark mode" --body "Decided to use system preference instead" | grep -oE '[0-9]+$')
gh pr close "$PR_NUM" --repo "$REPO"

make_branch "experiment/redis-cache" "cache.py" \
    'import redis
r = redis.Redis()' \
    "experiment: try redis for session cache"

PR_NUM=$(gh pr create --repo "$REPO" --head "experiment/redis-cache" \
    --title "Experiment: Redis caching" --body "Benchmarks showed memcached was faster" | grep -oE '[0-9]+$')
gh pr close "$PR_NUM" --repo "$REPO"

# -------------------------------------------------------------------
# OPEN PR — branch with an open pull request (preview-only in broom)
# -------------------------------------------------------------------
echo "==> Creating open PR branch..."

make_branch "feature/user-profiles" "profiles.py" \
    'def get_profile(user_id):
    return {"id": user_id, "name": "..."}' \
    "feat: add user profile endpoint"

gh pr create --repo "$REPO" --head "feature/user-profiles" \
    --title "Add user profiles" --body "WIP: user profile pages"

# -------------------------------------------------------------------
# NOPR — pushed to remote but no PR ever created
# -------------------------------------------------------------------
echo "==> Creating no-PR branches..."

make_branch "spike/graphql-api" "schema.graphql" \
    'type Query { user(id: ID!): User }' \
    "spike: prototype graphql schema"

make_branch "alook/debug-logging" "logging.conf" \
    '[loggers]
keys=root' \
    "add debug logging config"

# -------------------------------------------------------------------
# GONE — remote branch was deleted, local tracking branch remains
# -------------------------------------------------------------------
echo "==> Creating gone branches..."

make_branch "fix/null-pointer" "fix.py" \
    'if user is not None: process(user)' \
    "fix: guard against null user"
git push origin --delete "fix/null-pointer"

make_branch "chore/lint-config" ".eslintrc" \
    '{"extends": "standard"}' \
    "chore: add eslint config"
git push origin --delete "chore/lint-config"

make_branch "feature/onboarding-flow" "onboarding.py" \
    'STEPS = ["welcome", "setup", "done"]' \
    "feat: onboarding wizard steps"
git push origin --delete "feature/onboarding-flow"

# -------------------------------------------------------------------
# UNPUSHED — local-only branches, never pushed anywhere
# -------------------------------------------------------------------
echo "==> Creating unpushed branches..."

git checkout -b "scratch/benchmark-query" main
echo 'SELECT count(*) FROM users;' > bench.sql
git add bench.sql
git commit -m "scratch: benchmark user count query"
git checkout main

git checkout -b "idea/rate-limiting" main
echo 'RATE_LIMIT = "100/min"' > ratelimit.py
git add ratelimit.py
git commit -m "idea: sketch out rate limiting"
git checkout main

# -------------------------------------------------------------------
# Done
# -------------------------------------------------------------------
git checkout main
echo ""
echo "==> Demo repo ready at $DIR"
echo "    Remote: github.com/${REPO}"
echo ""
echo "    Branch counts:"
echo "      gone:     3  (fix/null-pointer, chore/lint-config, feature/onboarding-flow)"
echo "      unpushed: 2  (scratch/benchmark-query, idea/rate-limiting)"
echo "      nopr:     2  (spike/graphql-api, alook/debug-logging)"
echo "      closed:   2  (feature/dark-mode, experiment/redis-cache)"
echo "      merged:   3  (feature/add-login-page, fix/api-timeout, chore/update-deps)"
echo "      pr:       1  (feature/user-profiles)"
echo ""
echo "    Next steps:"
echo "      cd $DIR"
echo "      vhs docs/demo/preview.tape    # or wherever you copied the tape files"
