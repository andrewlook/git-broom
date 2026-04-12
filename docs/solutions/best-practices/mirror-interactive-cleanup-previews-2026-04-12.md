---
title: Mirror interactive cleanup workflows in preview modes
date: 2026-04-12
category: best-practices
module: git-broom
problem_type: best_practice
component: tooling
severity: medium
applies_when:
  - Building destructive CLI or TUI cleanup flows with a dry-run mode
  - Adding multiple review steps or tranches to an existing interactive tool
  - Showing protected or ineligible items that must stay visible during review
tags:
  - git-broom
  - cli
  - tui
  - dry-run
  - destructive-workflow
  - preview
---

# Mirror interactive cleanup workflows in preview modes

## Context

`git-broom` started as a single-tranche interactive cleanup tool, then grew into a multi-step workflow with `gone` and `unpushed` tranches, protected-branch handling, and per-tranche confirmation. Once the interactive flow became richer, the old flat `--dry-run` and `--batch` output stopped matching what users actually reviewed in the TUI, which made the preview less trustworthy.

## Guidance

Use one shared workflow model for both interactive and non-interactive paths, then render that model differently instead of inventing a separate preview abstraction.

In `git-broom`, the shared model lives in `src/app.rs`:

- `CleanupMode` defines each cleanup tranche and its explanation text.
- `Tranche` carries the branches for one step of the workflow.
- `scan_selected_modes()` returns the same tranche structure no matter how the command will be displayed.
- `Branch::display_name()` keeps protected labels such as `(current branch)` attached to the branch in every presentation.

Then keep the entry points in `src/main.rs` thin:

- `run_interactive()` walks the tranche list one step at a time.
- `run_dry_run()` and `run_batch()` both call `format_preview_lines()` so the preview uses the same tranche ordering and visibility rules as the interactive flow.
- `format_preview_title()` mirrors the title grammar from `render_title()` in `src/ui.rs`, including the tranche explanation and step count.

For destructive actions, preserve safety semantics across every output mode:

- Keep protected branches visible instead of filtering them out entirely.
- Show the same tranche names and explanations in preview output that the TUI shows in its title bar.
- Make abort behavior immediate and predictable (`Ctrl-C`, `Ctrl-D`, and `q` should all mean "leave without applying triage changes").
- Treat human readability as the default for preview output unless the product explicitly needs machine-oriented batch output.

## Why This Matters

Users treat `--dry-run` as a rehearsal for the real action. If the preview flattens the workflow, hides protected branches, or reorders what the TUI will show, users have to re-learn the tool once they enter interactive mode. That is especially risky for destructive cleanup commands because trust depends on being able to predict exactly what the next screen will do.

Using one tranche model also reduces drift inside the codebase. When the workflow changes, the scan logic, interactive flow, and preview output stay aligned because they are all derived from the same `CleanupMode` and `Tranche` data instead of parallel ad hoc formatting paths.

## When to Apply

- A CLI has both interactive review and non-interactive preview modes.
- The command can delete, mutate, or otherwise destroy user-owned state.
- Some items are eligible for action while others are visible but protected.
- The tool is adding new phases, categories, or step-by-step review screens over time.

## Examples

Before the preview alignment pass, the non-interactive output could collapse the workflow into a simple grouped list:

```text
gone (upstream branch no longer exists)
  feature/delete-me
```

After the alignment pass, the preview preserves the same mental model as the interactive screen:

```text
  git-broom   [gone: upstream branch no longer exists]                           (1/2)
  branch name                                              last commit             age
  -----------                                              -----------             ---
  feature/delete-me                          "remove stale branch flo..."    2 weeks ago
```

The same principle applies to protected branches. Instead of disappearing from preview output, they stay visible with their label:

```text
  feature/unpushed (current branch)          "experiment with local on..."   5 minutes ago
```

That matches the behavior in `src/ui.rs`, where protected rows are dimmed and non-deletable, and the logic in `src/app.rs`, where `toggle_delete()` opens a modal instead of allowing the current branch to be marked for deletion.

## Related

- `README.md` documents the user-facing tranche workflow and preview commands.
- `docs/archive/2026-04-10-feat-git-broom-interactive-branch-cleanup-plan.md` captures the original gone-only cleanup design.
- `docs/archive/2026-04-10-feat-git-broom-unpushed-mode-plan.md` captures the tranche expansion that introduced `unpushed`.
- GitHub issues: `#1` (gone mode), `#2` (unpushed mode), `#3` (closed mode follow-up).
