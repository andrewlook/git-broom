---
title: Preview-first grouped cleanup inventory with safe execution
date: 2026-04-12
last_updated: 2026-04-14
category: best-practices
module: git-broom
problem_type: best_practice
component: tooling
severity: medium
applies_when:
  - Designing a cleanup CLI or TUI that should default to a readable preview while keeping destructive work behind an explicit subcommand
  - Adding grouped review, cached metadata, persistent keep labels, and execution feedback to the same cleanup workflow
  - Keeping protected or ineligible branches visible and understandable across preview, review, and execution
tags:
  - git-broom
  - cli
  - tui
  - clean
  - destructive-workflow
  - preview
  - grouped-inventory
---

# Preview-first grouped cleanup inventory with safe execution

## Context

`git-broom` started as an interactive cleanup TUI, then grew into a grouped workflow with GitHub-backed branch categories, persistent keep labels, protected-branch handling, and step-by-step review. Once the product became something users wanted to browse repeatedly, preview stopped being a sidecar dry-run and became the main surface: bare `git-broom` now shows grouped branch inventory, while `git-broom clean` is the only destructive entrypoint.

That shift raises the bar for consistency. The grouped preview, interactive review, and execution phase all need to show the same branches, the same protection state, the same explanations, and the same command plan, or users stop trusting the tool.

## Guidance

Make the non-destructive grouped preview the default product surface, and make destructive cleanup an explicit second intent.

- In `src/main.rs`, `parse_cli()` routes bare `git-broom` plus legacy `--dry-run` / `--batch` aliases to preview mode.
- `git-broom clean` is the only destructive entrypoint and rejects preview-only groups such as `pr`.
- Preview defaults include the browse-only `pr` group so the default view covers the full local branch picture, not only deletion candidates.

Build preview, review, and execution from one shared grouped model instead of maintaining separate abstractions for “browse output” and “cleanup flow.”

In `git-broom`, that shared model lives in `src/app.rs`:

- `CleanupMode` defines each group, its explanation text, and whether it is cleanable.
- `CleanupGroup` carries one review step’s branches regardless of whether the user is previewing or cleaning.
- `scan_with_options()` returns the same group structure for preview and clean, including saved state, protections, and GitHub metadata.
- `App::from_group()` turns one group into the TUI state without re-deriving eligibility or visibility.

Keep row structure and branch semantics aligned across preview and TUI.

- Branch rows should use the same ordering rules everywhere: protected first, then saved, then regular cleanup candidates.
- Branch names should remain readable and visually primary.
- Protection state should read as inline metadata such as badges, not as part of the branch name itself.
- Secondary values should prioritize the right information for the group: PR URLs for GitHub-backed groups, truncated commit subjects for non-PR groups, compact ages inline for both.

Keep destructive execution in-window with progressive feedback.

- The review step should require explicit confirmation instead of treating bare Enter as approval.
- Execution should update the same command list the user just reviewed, rather than dropping into unrelated stdout output.
- The active command should have visible progress.
- Failures should remain on screen with the raw command output and a clear exit affordance so the user can fix the underlying problem outside the tool.

Persist workflow state under the git common directory instead of tracked files.

- Keep labels belong in repo-local metadata such as `keep-labels.json`.
- Cached GitHub metadata belongs beside that in `pr-cache.json`.
- Preview can reuse fresh cache for speed, but `git-broom clean` should refresh GitHub-backed metadata before destructive review.

## Why This Matters

Destructive tooling only feels safe when the preview answers the real question: “what will happen if I continue?” If preview, triage, and execution diverge in grouping, visibility, ordering, or wording, users have to mentally diff three different products while deciding whether to delete branches. That is exactly the moment when the tool should be doing the cognitive work for them.

The preview-first model also changes the role of the CLI. Once browsing becomes the primary use case, readability matters more than scriptability for default output, and keeping repeated-run state such as saved branches and cached PR metadata makes the tool materially more useful day to day.

## When to Apply

- A CLI can delete or mutate user-owned state but is used more often for browsing than for acting.
- The workflow includes multiple groups, protected items, or saved selections that must stay visible for context.
- Preview output needs to be trusted as a rehearsal for the destructive path.
- The tool benefits from cached metadata or user-owned repo-local preferences across repeated runs.

## Examples

Before the preview-first pass, destructive review and preview were separate concepts. After the refactor, grouped preview and grouped cleanup are the same model with different entrypoints:

```text
git-broom
git-broom --groups gone,closed
git-broom clean --groups nopr,gone
```

The preview should show the same branch set and the same row shape the TUI will use:

```text
PR (open pull request on GitHub)
  feat/cleanup-execution-spinner [current] 21m  https://github.com/andrewlook/git-broom/pull/11
```

Protected and saved branches stay visible in both preview and review instead of disappearing:

```text
UNPUSHED (no upstream tracking branch is configured)
  alook--rolloutApiJudgePrompt (saved) 2d  "chore: install mustache"
  alook--rawVendorOverride [worktree] 27m  "[buildkite] update buildkite-private image..."
```

Execution should also remain inside the same window the user just reviewed:

```text
Executing cleanup commands...
✓ git push origin :refs/heads/feature/old && git branch -D feature/old
/ git push origin :refs/heads/feature/next && git branch -D feature/next
```

And if a command fails, the user should not get kicked back to raw stdout immediately:

```text
Cleanup failed
command: git push origin :refs/heads/feature/old && git branch -D feature/old

error: failed to push some refs to '...'
```

That keeps the product honest: one grouped model, one review surface, one execution surface, and clear failure handling without forcing users to reconstruct context from a separate output mode.

## Related

- `README.md` documents the current preview-first CLI and grouped review behavior.
- `CHANGELOG.md` captures the user-facing changes that introduced the preview-first flow, grouped GitHub-backed inventory, and in-window execution feedback.
- `docs/archive/2026-04-10-feat-git-broom-interactive-branch-cleanup-plan.md` captures the original gone-only cleanup design.
- `docs/archive/2026-04-10-feat-git-broom-unpushed-mode-plan.md` captures the group-based expansion that introduced `unpushed`.
- GitHub issues: `#1` Interactive git branch cleanup TUI (v1 - gone mode), `#2` git-broom unpushed mode (v2), `#3` git-broom closed mode (v2/v3).
