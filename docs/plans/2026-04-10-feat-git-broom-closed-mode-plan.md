---
title: "feat: git-broom closed mode"
type: feat
date: 2026-04-10
---

# feat: git-broom closed mode (v2/v3)

## Overview

Add a `closed` mode to git-broom that identifies branches which still have a remote tracking branch, but whose GitHub PR is closed (without merge) or was never opened. These are leftover branches from reorganized PR stacks or abandoned work.

This is the most complex mode — it requires the `gh` CLI, network access, and handles remote branch deletion.

## Prerequisites

- v1 (`gone` mode) shipped and working
- Ideally `unpushed` mode shipped too (to validate the subcommand pattern)

## Proposed Solution

```
git-broom closed                 # only review closed/no-PR branches
git-broom gone unpushed closed   # review explicit tranches in order
git-broom closed --batch         # print branch names to stdout
git-broom closed --remote origin # specify remote (default: origin)
```

### Detection logic

1. Get all local branches with a remote tracking branch (from `git for-each-ref`)
2. Fetch all PRs in a single `gh` call:
   ```
   gh pr list --state all --json number,title,state,headRefName,url
   ```
3. For each branch, match by `headRefName`:
   - **PR found, state = "CLOSED"** → candidate for deletion
   - **No PR found** → candidate for deletion
   - **PR found, state = "OPEN"** → skip (active work)
   - **PR found, state = "MERGED"** → candidate (upstream should have been deleted, but wasn't)

### Delete scope

Both local and remote — `git push origin :BRANCH` then `git branch -D`.

**Deletion order: remote first, then local.** If remote deletion fails (permissions, network, branch protection), skip local deletion and report the error. This avoids losing the local ref when the remote is still alive.

### Additional context in TUI

- Branch name
- Last commit date
- PR status: `#142 closed`, `#98 merged`, `no PR`
- PR URL (shown in a detail line below the branch name)
- Tranche affordance text explaining what `closed` means in the cleanup workflow

### Pre-flight checks (beyond v1)

- `gh` is installed and on PATH
- `gh auth status` succeeds (user is authenticated)
- Remote exists (default `origin`, configurable via `--remote`)

### CLI flow

`closed` should fit into the same tranche workflow as `gone` and `unpushed`:

- default interactive mode reviews all implemented tranches in order
- explicit positional args limit the run to the requested tranches
- `--dry-run` groups results by tranche header

### New dependencies

| Crate | Purpose |
|---|---|
| `serde` + `serde_json` | Parsing `gh` JSON output |

### Edge cases

| Situation | Behavior |
|---|---|
| `gh` not installed | Error message: "gh CLI required for closed mode. Install from https://cli.github.com" |
| `gh` not authenticated | Error message: "Run `gh auth login` first" |
| GitHub API rate limited | Show partial results with warning, suggest retrying later |
| Remote branch already deleted | Skip with warning during deletion |
| Branch has open PR | Excluded from candidates |
| Remote deletion fails (branch protection) | Skip local deletion, report error |
| Multiple remotes | Use `--remote` flag, default to `origin` |
| Branch name differs from PR head ref | Match on `headRefName` from `gh` output, not branch name heuristics |

### Branch protection

Same as v1/v2 plus:
- Branches with **open** PRs are never shown for deletion
- Default branch detection via `gh repo view --json defaultBranchRef` (more reliable than hardcoding main/master)

## Acceptance Criteria

- [ ] `git-broom closed` shows branches with closed/no PRs in TUI
- [ ] PR status and URL shown per branch
- [ ] Remote branch deleted first, then local
- [ ] Graceful handling when `gh` is missing or unauthenticated
- [ ] `--batch` and `--dry-run` work with `closed` mode
- [ ] `closed` participates in the multi-tranche interactive workflow and grouped dry-run output
- [ ] `--remote` flag to specify non-origin remote
- [ ] Open PRs are excluded
- [ ] Protected branches are excluded

## Risks

| Risk | Mitigation |
|---|---|
| Remote branch deletion is irreversible | Remote-first order; confirmation step; `--dry-run` |
| `gh` API rate limits on repos with many PRs | Single batched API call, not per-branch |
| Stale PR data (PR closed then reopened between scan and confirm) | Accept this risk — the confirmation step is the safety net |
| User deletes branch for a PR that was intentionally closed (will reopen later) | Show PR title in TUI so user can make informed decision |
