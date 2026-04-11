---
title: "feat: Interactive git branch cleanup TUI"
type: feat
date: 2026-04-10
---

# feat: Interactive git branch cleanup TUI

## Overview

`git-broom` is a Rust TUI tool that helps developers clean up stale local git branches. It solves the problem where squash-merge workflows break `git branch --merged`, leaving dozens of zombie branches that are tedious to identify and clean up manually.

The tool works in two phases:
1. **Scan** — interactively review matching branches, mark each as keep or delete, output a TSV plan file
2. **Execute** — read the plan, confirm each deletion, then carry them out

## Problem Statement

In squash-merge PR workflows:
- `git branch --merged` doesn't detect squash-merged branches
- Upstream branches auto-delete after merge, but local tracking branches remain
- Prototype branches accumulate with no tracking branch
- Stale branches from reorganized PR stacks linger with closed/orphaned PRs

Developers must manually run `git branch -vv`, eyeball the output, and delete branches one by one. This is error-prone and tedious with 20+ branches.

## Proposed Solution

A two-command Rust TUI built with **ratatui** + **crossterm**:

```
git-broom scan <mode>    # interactive TUI → produces TSV plan
git-broom execute [file] # reads TSV → confirms + deletes
```

### Three scan modes

| Mode | Filter logic | Delete scope | External deps |
|---|---|---|---|
| `gone` | Upstream tracking branch deleted | local only | git |
| `unpushed` | No remote tracking branch configured | local only | git |
| `closed` | Remote exists, but PR is closed or never opened | local + remote | git + `gh` |

### Data flow

```
scan gone → TUI → .git/broom-plan.tsv → echo "run: git-broom execute"
                                              ↓
                                    execute → confirm each → git branch -D / git push origin :branch
```

## Technical Approach

### Project structure

```
src/
  main.rs       — CLI parsing (clap), entry point
  app.rs        — App state, update logic, branch data model
  ui.rs         — ratatui render functions (takes &App immutably)
  tui.rs        — Terminal setup/teardown wrapper with Drop cleanup
  scanner.rs    — Branch discovery: shells out to git + gh
  executor.rs   — Reads TSV, confirms, runs deletions
  plan.rs       — TSV read/write logic
```

### Key technical decisions

**Shell out to `git`, not `git2` crate.** Use `git for-each-ref` with a custom format string for machine-parseable output — no fragile string parsing. Keeps compile times fast and binary small. Matches user's git config.

```
git for-each-ref --format='%(refname:short)\t%(upstream:short)\t%(upstream:track)\t%(committerdate:iso8601)\t%(subject)' refs/heads/
```

**Shell out to `gh` CLI with `--json`.** Fetch all PRs once with `gh pr list --state all --json number,title,state,headRefName,url`, then match to branches in Rust. Avoids N+1 API calls.

**Sync event loop.** No async runtime needed. Load branch data before entering the TUI, then run a standard crossterm poll loop.

**Deletion order: remote first, then local.** For `closed` mode (scope: both), delete the remote branch first. If remote deletion fails, skip local deletion and report the error. This avoids the unrecoverable state where the local ref is gone but the remote branch persists.

### TUI design

```
┌─ git-broom: gone branches (3 of 17) ─────────────────────────┐
│                                                                │
│   >> ✗ feature/auth-refactor    2d ago   "refactor auth m..."  │
│      ✗ fix/cart-total           5d ago   "fix cart calcula..."  │
│      ✓ feature/new-api         12d ago   "add new API end..."  │
│      - experiment/fast-cache    3w ago   "try caching str..."  │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ j/k: navigate  d: delete  s: keep  a: all delete  q: quit     │
└────────────────────────────────────────────────────────────────┘
```

- `>>` highlights selected row
- `✗` = marked for deletion (red), `✓` = marked to keep (green), `-` = undecided (dim)
- Each row shows: branch name, relative commit date, truncated commit message
- For `closed` mode, also show PR status (e.g., `#142 closed`)
- Progress counter in title bar
- Full navigable list — user can go back and change decisions

### Keybindings

| Key | Action |
|---|---|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `d` | Mark for deletion |
| `s` | Mark to keep |
| `a` | Mark all for deletion |
| `u` | Unmark all |
| `Enter` | Finish and write TSV |
| `q` / `Esc` | Quit without saving |
| `?` | Show help overlay |

### TSV plan format

```tsv
branch	scope
feature/auth-refactor	local
fix/cart-total	local
old-stack/part-1	both
```

- Tab-separated, with header row
- `scope` is one of: `local`, `remote`, `both`
- Written to `.git/broom-plan.tsv` (inside `.git` so it's invisible to the project)
- Overwritten on each scan

### Execute phase

```
git-broom execute [--dry-run] [path]
```

- Defaults to `.git/broom-plan.tsv` if no path given
- Shows a summary table of planned deletions
- User confirms with `y` to proceed, or `n` to abort
- For each branch: execute deletion, print result (success/failure)
- Print summary at end: "Deleted 12 branches (3 local, 9 local+remote). 1 failed."
- `--dry-run` prints what would happen without doing it

### Pre-flight checks

Before scan or execute, validate:
- Running inside a git repo
- Not a bare repo
- Can identify current branch (handle detached HEAD)
- For `closed` mode: `gh` is installed and authenticated (`gh auth status`)
- For `closed` mode: remote `origin` exists (configurable via `--remote`)

### Branch protection

These branches are **never** shown for deletion:
- The currently checked-out branch (show with "current" label if it matches filter)
- Branches checked out in other worktrees (`git worktree list`)
- `main`, `master`, and the repo's default branch (detected via `gh repo view --json defaultBranchRef` or falling back to `main`/`master`)

### Edge cases handled

| Situation | Behavior |
|---|---|
| Zero matching branches | Print message, exit 0, no TUI |
| Current branch matches filter | Show in list with "(current)" label, forced to keep |
| Branch in another worktree | Show with "(worktree)" label, forced to keep |
| TSV references already-deleted branch | Skip with warning during execute |
| `gh` rate limited | Fail gracefully, show partial results with warning |
| Remote deletion fails, local succeeds | N/A — remote deleted first; on failure, local skipped |
| Branch name with slashes | Handled naturally by git commands |
| 100+ matching branches | Bulk keybindings (`a` mark all, `u` unmark all) |

## Acceptance Criteria

- [ ] `git-broom scan gone` shows branches whose upstream was deleted, user marks keep/delete, writes TSV
- [ ] `git-broom scan unpushed` shows branches with no remote tracking branch
- [ ] `git-broom scan closed` shows branches with closed/no PRs (requires `gh`)
- [ ] `git-broom execute` reads TSV, confirms with user, deletes branches
- [ ] `git-broom execute --dry-run` shows what would be deleted without acting
- [ ] Current branch and worktree branches are protected from deletion
- [ ] `main`/`master`/default branch are protected
- [ ] Tool echoes next command after scan phase completes
- [ ] Works on macOS and Linux
- [ ] Published to crates.io (`cargo install git-broom`)
- [ ] Homebrew tap available (`brew install <user>/tap/git-broom`)

## Dependencies & Risks

### Dependencies

| Crate | Purpose |
|---|---|
| `ratatui` ~0.29 | TUI framework |
| `crossterm` ~0.28 | Terminal backend |
| `clap` 4 (derive) | CLI argument parsing |
| `serde` + `serde_json` | Parsing `gh` JSON output |
| `anyhow` | Error handling |

External tools: `git` (required), `gh` (required only for `closed` mode).

### Risks

| Risk | Mitigation |
|---|---|
| Remote branch deletion is irreversible | Remote-first deletion order; `--dry-run` flag; confirmation step |
| `gh` API rate limits on large repos | Batch fetch all PRs in one call, not per-branch |
| Stale TSV (branches change between scan and execute) | Execute validates each branch exists before deleting |
| Terminal not restored on panic | `Drop`-based cleanup + panic hook that restores terminal |

### Distribution

Use **cargo-dist** for automated cross-compilation, GitHub Releases, and Homebrew formula generation:
- `cargo dist init` to scaffold CI
- GitHub Actions matrix: `x86_64-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`
- Auto-generated Homebrew formula in a `homebrew-tap` repo

## References

- ratatui docs: https://docs.rs/ratatui/latest/ratatui/
- crossterm docs: https://docs.rs/crossterm/latest/crossterm/
- `git for-each-ref` format strings: `git help for-each-ref`
- `gh pr list --json` fields: `gh pr list --help`
- cargo-dist: https://opensource.axo.dev/cargo-dist/
