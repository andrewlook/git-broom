# git-broom

Squash-merge workflows leave stale tracking branches behind. You can nuke whole categories with one-liners like `git branch --merged | xargs git branch -d`, but that's often too aggressive — some branches have unpushed work, some track PRs you still care about. `git-broom` groups your branches by remote and PR status so you can see what's worth keeping at a glance, then lets you interactively triage and clean up the rest.

- **`git-broom`** — preview branches grouped by status (safe, read-only)
- **`git-broom clean`** — enter the interactive TUI to triage and delete branches

[![asciicast](https://asciinema.org/a/JTaehSN1oYjpxTPq.svg)](https://asciinema.org/a/JTaehSN1oYjpxTPq)

## Branch Groups

| Group | Has remote? | PR status | Equivalent one-liner |
|-------|:-----------:|-----------|----------------------|
| `gone` | ❌ (was tracked, now deleted) | — | `git branch -vv \| grep ': gone]'` |
| `unpushed` | ❌ (never pushed) | — | `git branch -vv \| grep -v '\[.*/'` |
| `pr` | ✅ | 🟢 Open | `gh pr list --author @me` |
| `nopr` | ✅ | ⚪ None | _(no simple one-liner)_ |
| `closed` | ✅ | 🔴 Closed | `gh pr list --state closed --author @me` |
| `merged` | ✅ | 🟣 Merged (remote branch still exists) | `gh pr list --state merged --author @me` |

By default, `git-broom` previews all six groups. `git-broom clean` uses all except `pr` (open PRs are preview-only).

## Install

You need [Rust](https://rustup.rs) and the [GitHub CLI](https://cli.github.com) (`gh`):

```bash
# authenticate with GitHub (needed for pr/nopr/closed/merged groups)
gh auth login

# install from crates.io
cargo install git-broom
```

## Usage

### Preview branches

```bash
# preview all groups
git-broom

# preview only specific groups
git-broom -g gone,unpushed
git-broom -g pr,closed,merged

# use a different remote (default: origin)
git-broom -g merged --remote upstream
```

### Clean up branches

```bash
# interactively triage all cleanup groups
git-broom clean

# only triage specific groups
git-broom clean -g gone,unpushed
```

## Interactive TUI

`git-broom clean` opens a terminal UI with two screens:

### Triage screen

Browse branches in the current group and decide what to do with each one.

| Key | Action |
|-----|--------|
| `j` / `k` or `↑` / `↓` | Navigate branches |
| `d` | Toggle branch for deletion |
| `s` | Toggle save (persists across sessions — saved branches are skipped by delete-all) |
| `a` | Mark all deletable branches for deletion |
| `u` | Clear all delete marks |
| `Enter` | Proceed to review screen |
| `q` / `Esc` | Quit |

Protected branches (`main`, `master`, the current branch, worktree checkouts) are shown for context but cannot be deleted.

### Review screen

Confirm the branches you marked for deletion.

| Key | Action |
|-----|--------|
| `y` | Confirm and delete |
| `n` | Go back to triage |
| `q` / `Esc` | Quit |

## How It Works

- **Preview mode** shows a grouped branch list and exits — nothing is modified.
- **Clean mode** walks each group through the TUI, then deletes confirmed branches:
  - For `gone` and `unpushed`: runs `git branch -D <branch>`
  - For `nopr`, `closed`, `merged`: deletes the remote branch (`git push <remote> :refs/heads/<branch>`) then the local branch
- **Saved branches** are stored per-repo under `.git/git-broom/keep-labels.json`. They stay visible in preview and TUI output but are excluded from delete-all until you unsave them.
- **PR metadata** is cached at `.git/git-broom/pr-cache.json` to speed up repeated previews. `git-broom clean` always refreshes GitHub data before destructive review.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, testing, and release instructions.

## License

MIT. See [LICENSE](LICENSE).
