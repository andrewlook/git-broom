#!/usr/bin/env bash
set -euo pipefail

if ! command -v gh >/dev/null 2>&1; then
  echo "gh is required to draft release notes from merged PRs." >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

VERSION="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
BASE_BRANCH="${2:-main}"

LAST_TAG="$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null || true)"
if [[ -n "${LAST_TAG}" ]]; then
  SINCE_DATE="$(git log -1 --format=%cI "${LAST_TAG}")"
  RANGE_LABEL="since ${LAST_TAG}"
else
  SINCE_DATE="$(git log --reverse --format=%cI | head -1)"
  RANGE_LABEL="since the first commit"
fi

TODAY="$(date +%F)"

cat <<EOF
## [${VERSION}] - ${TODAY}

### Added

- TODO

### Changed

- TODO

### Fixed

- TODO

### Merged PRs ${RANGE_LABEL}

EOF

gh pr list \
  --state merged \
  --base "${BASE_BRANCH}" \
  --search "merged:>=${SINCE_DATE}" \
  --limit 200 \
  --json number,title,url \
  --template '{{range .}}{{printf "- #%v %s (%s)\n" .number .title .url}}{{end}}'
