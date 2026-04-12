---
title: "feat: Interactive git branch cleanup TUI"
type: feat
date: 2026-04-10
---

# feat: Interactive git branch cleanup TUI (v1)

## Overview

`git-broom` is a Rust TUI tool that helps developers clean up stale local git branches. It solves the problem where squash-merge workflows break `git branch --merged`, leaving dozens of zombie branches that are tedious to identify and clean up manually.

v1 focuses on the most common case: branches whose upstream tracking branch has been deleted (i.e., already merged via squash-merge).

## Problem Statement

In squash-merge PR workflows:
- `git branch --merged` doesn't detect squash-merged branches
- Upstream branches auto-delete after merge, but local tracking branches remain
- Developers must manually run `git branch -vv`, eyeball the output, and delete branches one by one

## Proposed Solution

A single-command Rust TUI built with **ratatui** + **crossterm**:

```
git-broom           # interactive TUI → mark branches → confirm → delete
git-broom --batch   # print deletable branch names to stdout (scriptable)
git-broom --dry-run # show what would be deleted without acting
```

v1 handles "gone" branches only — branches whose upstream tracking branch has been deleted. Future modes (`unpushed`, `closed`) are planned separately.

### Data flow

```
git-broom → scan (with spinner) → TUI (mark keep/delete) → confirm → delete → summary
```

No intermediate files. Scan, decide, and execute in a single session.

## Technical Approach

### Project structure

```
src/
  main.rs       — CLI args, terminal setup/teardown, entry point
  app.rs        — App state, branch scanning, update logic, deletion
  ui.rs         — ratatui render functions (takes &App immutably)
```

### Key technical decisions

**Shell out to `git`, not `git2` crate.** Use `git for-each-ref` with a custom format string for machine-parseable output — no fragile string parsing. Keeps compile times fast and binary small. Matches user's git config.

```
git for-each-ref --format='%(refname:short)\t%(upstream:short)\t%(upstream:track)\t%(committerdate:iso8601)\t%(subject)' refs/heads/
```

Filter for branches where `%(upstream:track)` contains `[gone]`.

**Sync event loop.** No async runtime needed. Run the scan before entering the TUI (with a progress message on stdout), then enter the interactive loop.

**Scan before TUI.** Run `git for-each-ref` and collect results before entering ratatui's alternate screen. Show a "Scanning branches..." message on stdout during the scan. If zero branches match, print a message and exit — no TUI needed.

### TUI design

```
┌─ git-broom: 3 of 17 gone branches ───────────────────────────┐
│                                                                │
│   >> ✗ feature/auth-refactor    2d ago   "refactor auth m..."  │
│      ✗ fix/cart-total           5d ago   "fix cart calcula..."  │
│      ✓ feature/new-api         12d ago   "add new API end..."  │
│      - experiment/fast-cache    3w ago   "try caching str..."  │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ j/k: navigate  d: delete  s: keep  a: all delete  Enter: go   │
└────────────────────────────────────────────────────────────────┘
```

- `>>` highlights selected row
- `✗` = marked for deletion (red), `✓` = marked to keep (green), `-` = undecided (dim)
- Each row shows: branch name, relative commit date, truncated commit message
- Progress counter in title bar
- Full navigable list — user can go back and change decisions
- Keybindings shown in the footer — no help overlay needed

### Keybindings

| Key | Action |
|---|---|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `d` | Mark for deletion |
| `s` | Mark to keep |
| `a` | Mark all for deletion |
| `u` | Unmark all |
| `Enter` | Confirm and execute |
| `q` / `Esc` | Quit without deleting |

### Confirmation flow

When the user presses `Enter`:
1. Exit TUI (leave alternate screen)
2. Print a summary: "About to delete N branches:" followed by the list
3. Prompt `Proceed? [y/N]`
4. On `y`: run `git branch -D` for each, print results
5. Print summary: "Deleted 12 branches. 1 failed."

### `--batch` mode

For scripting/composability:

```
git-broom --batch
```

Prints one branch name per line to stdout. No TUI, no confirmation. Composable:

```
git-broom --batch | xargs git branch -D
```

### `--dry-run` mode

```
git-broom --dry-run
```

Runs the scan and prints matching branches with their metadata, but does not enter the TUI or delete anything.

### Pre-flight checks

Before scanning, validate:
- Running inside a git repo (check for `.git`)
- Not a bare repo
- Can identify current branch (handle detached HEAD gracefully)

### Branch protection

These branches are **never** shown for deletion:
- The currently checked-out branch (show with "(current)" label if it matches filter)
- Branches checked out in other worktrees (`git worktree list --porcelain`)
- `main` and `master`

### Edge cases

| Situation | Behavior |
|---|---|
| Zero matching branches | Print "No gone branches found.", exit 0 |
| Current branch matches filter | Show in list with "(current)" label, forced to keep |
| Branch in another worktree | Show with "(worktree)" label, forced to keep |
| `git branch -D` fails | Print error, continue with remaining branches |
| 100+ matching branches | Bulk keybindings (`a` mark all, `u` unmark all) |
| Detached HEAD | No current branch to protect, scanning works normally |

## Acceptance Criteria

- [ ] `git-broom` shows branches whose upstream was deleted, user marks keep/delete, confirms, branches are deleted
- [ ] `git-broom --batch` prints gone branch names to stdout
- [ ] `git-broom --dry-run` prints matching branches without deleting
- [ ] Current branch, worktree branches, and main/master are protected
- [ ] Zero matching branches prints a message and exits cleanly
- [ ] Terminal is always restored on exit, including panics
- [ ] Works on macOS and Linux
- [ ] Integration tests using temp git repos
- [ ] Published to crates.io (`cargo install git-broom`)
- [ ] Homebrew tap available (`brew install <user>/tap/git-broom`)

## Dependencies

| Crate | Purpose |
|---|---|
| `ratatui` ~0.29 | TUI framework |
| `crossterm` ~0.28 | Terminal backend |
| `anyhow` | Error handling |

External tools: `git` (required).

No `clap` — parse `std::env::args()` for `--batch` and `--dry-run`. Add clap later when subcommands arrive (v2 modes).

No `serde` / `serde_json` — not needed until `closed` mode (v2).

## Risks

| Risk | Mitigation |
|---|---|
| `git branch -D` is destructive | Confirmation step; commits remain in reflog ~30 days |
| Terminal not restored on panic | `Drop`-based cleanup + panic hook |
| `git for-each-ref` format changes | Format is stable and documented; low risk |

## Distribution

Use **cargo-dist** for automated cross-compilation, GitHub Releases, and Homebrew formula generation:
- `cargo dist init` to scaffold CI
- GitHub Actions matrix: `x86_64-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`
- Auto-generated Homebrew formula in a `homebrew-tap` repo

## Testing strategy

- **Unit tests**: branch parsing logic, branch protection filtering
- **Integration tests**: create temp git repos with `git init`, add branches with various tracking states, run the scanner, assert correct branches are identified
- **TUI tests**: not in v1 — test the logic, not the rendering

## Future work

- `unpushed` mode — see `docs/archive/2026-04-10-feat-git-broom-unpushed-mode-plan.md`
- `closed` mode — see `docs/archive/2026-04-10-feat-git-broom-closed-mode-plan.md`

## References

- ratatui docs: https://docs.rs/ratatui/latest/ratatui/
- crossterm docs: https://docs.rs/crossterm/latest/crossterm/
- `git for-each-ref` format strings: `git help for-each-ref`
- cargo-dist: https://opensource.axo.dev/cargo-dist/
