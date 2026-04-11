---
title: "feat: git-broom unpushed mode"
type: feat
date: 2026-04-10
---

# feat: git-broom unpushed mode (v2)

## Overview

Add an `unpushed` mode to git-broom that identifies local branches with no remote tracking branch configured. These are prototype branches that were never pushed — experiments, scratch work, or abandoned ideas.

## Prerequisites

- v1 (`gone` mode) shipped and working

## Proposed Solution

```
git-broom                   # review implemented cleanup tranches in sequence
git-broom unpushed          # only review local branches with no remote tracking
git-broom gone unpushed     # review two explicit tranches in order
git-broom gone unpushed --dry-run
```

### Detection logic

From the same `git for-each-ref` output used in v1, filter for branches where `%(upstream:short)` is empty — no upstream configured at all.

```
git for-each-ref --format='%(refname:short)\t%(upstream:short)\t%(upstream:track)\t%(committerdate:iso8601)\t%(subject)' refs/heads/
```

Branches with an empty second column have no remote tracking branch.

### Delete scope

Local only — `git branch -D`. There is no remote branch to delete.

### Additional context in TUI

Since these branches were never pushed, the user may not remember what they are. Show:
- Branch name
- Last commit date (relative)
- Last commit message (truncated)
- Number of commits ahead of HEAD (helps gauge how much work is on the branch)

### CLI changes

With the addition of multiple cleanup tranches, the CLI should accept one or more positional mode names plus flags:

```
git-broom                    # default workflow: review all implemented tranches in order
git-broom gone               # only review gone branches
git-broom unpushed           # only review unpushed branches
git-broom gone unpushed      # explicit tranche order
git-broom <mode>... --batch
git-broom <mode>... --dry-run
```

The default (no explicit modes) should review all implemented tranches in order. Interactive mode confirms each tranche independently.

`--dry-run` should group output by tranche, for example:

```
gone (upstream branch no longer exists)
  feature/auth-cleanup                     (14 days ago)

unpushed (no upstream tracking branch is configured)
  experiment/try-ratatui                   (7 weeks ago)
```

### Branch protection

Same as v1: protect current branch, worktree branches, `main`, `master`.

## Acceptance Criteria

- [ ] `git-broom unpushed` shows branches with no remote tracking in TUI
- [ ] User can mark keep/delete and confirm deletions
- [ ] `--batch` and `--dry-run` work with `unpushed` mode
- [ ] Default interactive mode reviews all implemented tranches in sequence
- [ ] `git-broom gone unpushed --dry-run` groups results by tranche with descriptive headers
- [ ] Protected branches are excluded

## Risks

| Risk | Mitigation |
|---|---|
| Unpushed branches may contain important uncommitted work | Show commit count and last commit message prominently |
| User confusion between modes | Clear tranche label + short description in the interactive flow and dry-run headers |
