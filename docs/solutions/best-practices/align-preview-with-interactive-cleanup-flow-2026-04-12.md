---
title: "Keep destructive CLI previews aligned with the interactive cleanup flow"
date: 2026-04-12
category: best-practices
module: git-broom
problem_type: best_practice
component: tooling
severity: medium
applies_when:
  - adding dry-run or batch output for an interactive destructive workflow
  - reviewing branch cleanup in ordered tranches or steps
  - some visible candidates are intentionally ineligible for deletion
tags: [cli-ux, tui, dry-run, destructive-actions, branch-cleanup, preview-parity, cleanup-tranches]
---

# Keep destructive CLI previews aligned with the interactive cleanup flow

## Context

`git-broom` started as a single-mode TUI for deleting `gone` branches. Once the tool expanded into a tranche-based workflow with `gone` and `unpushed` modes, the review model became more structured: scan once, review each tranche in order, protect current/worktree/default branches, then confirm deletion one tranche at a time.

The first pass at `--dry-run` and `--batch` drifted away from that model. The non-interactive output looked like a separate report instead of a preview of the same review flow, and protected rows could disappear from the preview even though they were still visible in the interactive screen.

## Guidance

When a CLI has a destructive interactive workflow, make the non-interactive preview reflect the same mental model instead of inventing a second one.

In `git-broom`, that means three things:

1. Scan once into a shared inventory and derive ordered `Tranche` values from it.
2. Reuse tranche order and branch metadata for both the TUI and the preview output.
3. Keep protected rows visible in preview output with inline labels instead of silently filtering them out.

The shared scan happens in `scan_selected_modes` in `src/app.rs`, and the top-level command dispatch keeps all output modes on that same data set:

```rust
let tranches = scan_selected_modes(&repo, &cli.modes)?;

match cli.output {
    OutputMode::Interactive => run_interactive(&repo, tranches),
    OutputMode::Batch => run_batch(&tranches),
    OutputMode::DryRun => run_dry_run(&tranches),
}
```

The preview formatter in `src/main.rs` mirrors the TUI structure instead of printing a flat list:

```rust
lines.push(format_preview_title(
    tranche.mode,
    step_index,
    step_count,
    total_width,
));
lines.push(format_preview_header(branch_width, commit_width, age_width));
lines.push(format_preview_rule(branch_width, commit_width, age_width));
lines.extend(
    tranche
        .branches
        .iter()
        .map(|branch| format_preview_branch(branch, branch_width, commit_width, age_width)),
);
```

Protected branches stay visible because preview rows use `branch.display_name()`, which includes labels like `(current branch)` from `src/app.rs`. Interactive mode enforces the same protection by opening a modal instead of toggling delete when `App::toggle_delete` hits a protected row.

## Why This Matters

Preview parity is part of the safety model for destructive tools.

- Users trust the tool more when `--dry-run` looks like the screen they will actually review.
- Step counts stay stable when protected rows are shown instead of disappearing from preview output.
- Adding new cleanup modes stays cheaper because the workflow is centered on shared tranche data, not bespoke per-mode rendering paths.
- For a human-oriented cleanup tool, readable preview output is more valuable than script-friendly one-item-per-line output.

## When to Apply

- A CLI or TUI lets users mark items for deletion and also offers `--dry-run`.
- The workflow is multi-step or grouped into review tranches.
- Some rows must stay visible for context even though they are not eligible for deletion.
- User feedback says the preview feels like a separate report instead of a preflight check for the real workflow.

## Examples

Before this pattern:

- interactive mode reviewed `gone` and `unpushed` branches as separate steps
- `--dry-run` had a different presentation model
- `--batch` optimized for scriptability instead of readability
- protected rows could vanish from preview output, making the review flow feel inconsistent

After this pattern:

- `run_interactive` in `src/main.rs` reviews the selected tranches in order
- `render_title` in `src/ui.rs` and `format_preview_title` in `src/main.rs` both show the same step-oriented context
- `render_branch` in `src/ui.rs` and `format_preview_branch` in `src/main.rs` use the same branch name / last commit / age structure
- current and other protected branches remain visible via `Branch::display_name()` in `src/app.rs`

This made output like `git-broom gone unpushed --dry-run` behave like a real preflight review:

```text
  git-broom   [gone: upstream branch no longer exists]                           (1/2)
  branch name                                              last commit             age
  -----------                                              -----------             ---
  feature/gone                            "gone branch commit subje...    1 second ago

  git-broom   [unpushed: no upstream tracking branch is configured]              (2/2)
  branch name                                              last commit             age
  -----------                                              -----------             ---
  feature/unpushed (current branch)       "unpushed branch commit s...    1 second ago
```

## Related

- `README.md`
- `src/main.rs`
- `src/app.rs`
- `src/ui.rs`
- `tests/integration.rs`
- `docs/archive/2026-04-10-feat-git-broom-unpushed-mode-plan.md`
- GitHub issue #1: https://github.com/andrewlook/git-broom/issues/1
