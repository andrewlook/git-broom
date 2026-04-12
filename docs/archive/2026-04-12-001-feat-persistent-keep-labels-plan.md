---
title: feat: Add persistent keep labels to cleanup review
type: feat
status: archived
date: 2026-04-12
---

# feat: Add persistent keep labels to cleanup review

## Overview

Add a persistent "keep" label to `git-broom` so users can explicitly save branches during cleanup review, carry that decision across future runs, and see saved branches separated from normal deletion candidates in both the TUI and preview output.

## Problem Frame

`git-broom` currently distinguishes only between "not marked for deletion" and "marked for deletion" during a single run. That is not enough for repeated cleanup workflows: users often revisit the same repo many times and want to permanently exclude specific branches from routine cleanup even when they continue to qualify for a cleanup group. The new behavior needs to preserve destructive-flow trust:

- saved branches must be visibly distinct from ordinary candidates
- saved state must persist per repo without creating tracked files
- interactive and preview modes must show the same ordering and segmentation
- saved branches must not become easier to delete accidentally than regular branches

## Requirements Trace

- R1. Users can explicitly mark an eligible branch as "keep" during interactive review, distinct from simply leaving it undecided.
- R2. Keep labels persist per repo across future command runs without writing tracked files into the worktree.
- R3. Each cleanup group displays branches in this order: protected/current/worktree first, saved second, regular candidates last.
- R4. Protected and saved sections are visually distinct from regular candidates, and the same segmentation appears in `--dry-run` / `--batch`.
- R5. The initial TUI selection starts at the first regular candidate, while still allowing navigation into protected and saved sections.
- R6. Removing a keep label returns the branch to the regular section without automatically marking it for deletion.
- R7. Saved branches are not included in bulk delete affordances until the user explicitly removes the keep label.

## Scope Boundaries

- No new cleanup modes or CLI positional modes are added.
- No cross-repo or global keep store is added; persistence is local to one git clone.
- No attempt is made to sync keep labels across different clones of the same repo.
- No standalone management command for listing or clearing saved labels is added in this pass.

## Context & Research

### Relevant Code and Patterns

- `src/app.rs` already owns branch scanning, cleanup-group construction, protection rules, and per-row decisions.
- `App::from_group()` in `src/app.rs` is the right place to establish initial selection behavior for each review step.
- `src/ui.rs` renders rows directly from `App.branches`; section ordering or segmentation should be derived from app-layer state rather than duplicated in the UI.
- `format_preview_lines()` in `src/main.rs` already mirrors the interactive workflow structure for preview modes.
- `tests/integration.rs` already uses temp repos plus fake `gh` helpers to verify cleanup-mode behavior end to end.

### Institutional Learnings

- `docs/solutions/best-practices/mirror-interactive-cleanup-previews-2026-04-12.md` reinforces that destructive preview modes should mirror the interactive workflow, not invent a separate reporting abstraction.

### External References

- External research skipped. The repo already has strong local patterns for destructive workflow modeling, preview parity, and git-aware temp-repo integration testing.

## Key Technical Decisions

- Persist keep labels under the repo's git common directory, not the working tree.
  Rationale: `git rev-parse --git-common-dir` resolves a safe per-repo storage location that is not committed and can be shared across worktrees of the same clone. The persisted file should live at `<git-common-dir>/git-broom/keep-labels.json`.

- Store keep labels by cleanup mode plus branch name, not by branch name alone.
  Rationale: current cleanup groups are mostly disjoint, but mode-scoped labels avoid surprising future overlap and match the user's mental model of "save this branch from this cleanup group."

- Treat unreadable persisted keep state as a hard error, not a silent fallback.
  Rationale: silently dropping saved labels would re-expose branches the user intentionally protected from cleanup. Failing closed is safer for a destructive workflow.

- Keep labels participate in section ordering but do not replace protection rules.
  Rationale: protected/current/worktree branches remain the highest-priority section. If a branch is both protected and saved, the protected section wins visually and behaviorally.

- Saved branches require an explicit unsave action before deletion.
  Rationale: this preserves the distinction between "I want to keep this over time" and "I have not decided yet." Bulk delete should skip saved rows.

- Use an interactive save toggle key (`s`) rather than overloading delete or clear actions.
  Rationale: `d` already means delete toggle, and `u` already clears delete selections. A dedicated key avoids accidental destructive transitions and supports the requested "remove keep label, then return to regular list" workflow.

## Open Questions

### Resolved During Planning

- **Where should keep labels live?**
  Use `<git-common-dir>/git-broom/keep-labels.json`, derived from git metadata rather than hardcoding `.git/`.

- **How should group ordering work when a branch is protected and saved?**
  Use precedence `protected > saved > regular`.

- **What action removes a keep label?**
  `s` toggles keep on/off for eligible branches. Removing keep returns the row to the regular section with `Decision::Undecided`.

- **How should delete-all interact with saved branches?**
  `a` affects regular deletable rows only. Saved rows are excluded until unsaved.

### Deferred to Implementation

- **Should preview mode add explicit textual section labels like `saved` / `protected`, or rely on blank-line segmentation plus inline row styling/labels?**
  The implementation can choose the lightest presentation that stays readable in narrow terminals, as long as the three sections remain obvious and ordered.

- **Should stale keep labels for deleted branches be pruned opportunistically on load or only when the store is rewritten?**
  Either is acceptable in this pass as long as stale entries do not appear in UI/preview output and do not corrupt future saves.

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for review, not implementation specification. The implementing agent should treat it as context, not code to reproduce.*

| Branch state | Section | Delete key | Save key | Bulk delete |
|---|---|---|---|---|
| Protected (`current`, `worktree`, etc.) | protected | ineligible modal | optional no-op | excluded |
| Saved + deletable | saved | requires unsave first | toggles to regular | excluded |
| Regular + deletable | regular | toggles delete | toggles to saved | included |

Directional flow:

1. Scan branches into `CleanupGroup` as today.
2. Load persisted keep labels for the current repo.
3. Annotate each branch with `saved` state for the current cleanup mode.
4. Sort branches into `protected`, `saved`, `regular`, preserving existing age ordering within each section.
5. Render the same ordered list in TUI and preview, inserting blank lines at section boundaries.
6. Initialize TUI selection to the first regular row, falling back to the first saved row, then first protected row, then first row overall.

## Implementation Units

- [x] **Unit 1: Add repo-local keep label persistence**

**Goal:** Introduce a durable store for per-repo keep labels that is safe for normal repos and worktrees.

**Requirements:** R1, R2

**Dependencies:** None

**Files:**
- Create: `src/keep_store.rs`
- Modify: `src/lib.rs`
- Modify: `src/app.rs`
- Test: `tests/integration.rs`

**Approach:**
- Add a small persistence module responsible for:
  - resolving the git common directory
  - reading and writing a JSON keep-label file
  - exposing mode-scoped lookups such as "is this branch saved for this mode?"
- Keep the storage schema simple and versioned so future evolution is possible without rewriting app logic.
- Ensure missing file/directory is treated as empty state.
- Ensure malformed JSON or inaccessible store path returns an actionable error rather than silently ignoring saved labels.
- Remove the persisted file when the last saved label is cleared, so the repo does not accumulate empty local metadata.

**Patterns to follow:**
- Keep git-aware repo inspection in the same style as existing helpers in `src/app.rs`.
- Use temp-repo integration tests in `tests/integration.rs` rather than mocking file behavior in isolation only.

**Test scenarios:**
- Happy path: saving a keep label writes a repo-local file under the git common dir and the next run reloads it.
- Happy path: separate labels for the same branch name in different cleanup modes do not conflict.
- Edge case: missing keep-label file loads as empty state without error.
- Error path: malformed keep-label JSON aborts with an actionable error message that identifies the file path.
- Integration: when running from a normal repo clone, the resolved storage path is under `.git/`; when using git-common-dir semantics later, the same resolver remains valid.

**Verification:**
- An implementer can save a branch, rerun `git-broom`, and see the branch appear in the saved section without creating tracked files in the worktree.

- [x] **Unit 2: Extend app state with persisted keep labels and section ordering**

**Goal:** Model saved branches distinctly from per-run delete decisions and reorder each cleanup group into protected, saved, and regular sections.

**Requirements:** R1, R3, R5, R6, R7

**Dependencies:** Unit 1

**Files:**
- Modify: `src/app.rs`
- Test: `tests/integration.rs`

**Approach:**
- Extend `Branch` with persisted keep metadata separate from `Decision`.
- Introduce a derived section concept such as `BranchSection::{Protected, Saved, Regular}` in app-layer code.
- Reorder `CleanupGroup.branches` after keep labels are applied, preserving existing age-based ordering within each section.
- Update `App::from_group()` so the initial cursor lands on the first regular row; if there are no regular rows, fall back predictably.
- Keep `delete_candidates()`, `mark_all_delete()`, and related helpers scoped to regular deletable rows only.
- Decide how `d` behaves on saved rows; preferred behavior is a modal instructing the user to remove the keep label first, mirroring existing protected-branch affordances.

**Execution note:** Start with characterization coverage around current branch ordering and cursor initialization before changing the grouping behavior.

**Patterns to follow:**
- Existing protection handling and modal behavior in `src/app.rs`
- Existing step-based cleanup-group model in `CleanupGroup` and `App`

**Test scenarios:**
- Happy path: a saved branch in a group appears after protected rows and before regular rows.
- Happy path: unsaving a branch moves it into the regular section with no delete marker.
- Edge case: a saved branch that is also protected appears in the protected section, not the saved section.
- Edge case: when a group contains only protected and saved rows, initial selection falls back deterministically and navigation still reaches every row.
- Error path: `mark_all_delete()` skips saved rows and protected rows.
- Integration: rerunning `git-broom` against the same temp repo reproduces the same saved ordering from persisted state.

**Verification:**
- Branch ordering is stable across runs and matches the requested section precedence without regressing existing protection behavior.

- [x] **Unit 3: Add interactive save / unsave affordances**

**Goal:** Let users toggle persisted keep labels in the TUI and clearly understand which rows are protected, saved, or regular.

**Requirements:** R1, R4, R5, R6, R7

**Dependencies:** Unit 2

**Files:**
- Modify: `src/main.rs`
- Modify: `src/ui.rs`
- Modify: `src/app.rs`
- Test: `tests/integration.rs`

**Approach:**
- Add an `s` keybinding to toggle keep for eligible rows.
- Update footer hints and any branch-state indicators so saved rows are clearly recognizable.
- Style saved rows greenish in the TUI, while retaining existing protected-row dimming and delete-row strike-through behavior.
- Insert blank visual separators between protected, saved, and regular sections.
- Keep navigation global across the full list so users can move upward and unsave a previously saved row.
- If `d` is pressed on a saved row, show a short modal explaining that the row must be unsaved first.

**Patterns to follow:**
- Existing modal flow for protected rows in `src/app.rs`
- Existing keybinding / footer presentation in `src/main.rs` and `src/ui.rs`

**Test scenarios:**
- Happy path: pressing `s` on a regular eligible row marks it saved and moves it into the saved section.
- Happy path: pressing `s` again on a saved row unsaves it and returns it to the regular section without marking delete.
- Edge case: initial selection starts after protected/saved sections when regular rows exist.
- Error path: pressing `d` on a saved row shows guidance instead of marking it for deletion.
- Integration: a TTY smoke test can navigate into the saved section, unsave a branch, and return to normal deletion triage.

**Verification:**
- The interactive workflow exposes a stable, explicit save/unsave loop without conflating saved state with delete state.

- [x] **Unit 4: Mirror saved-section behavior in preview output and docs**

**Goal:** Keep `--dry-run` / `--batch` aligned with the new interactive grouping and document the persistent keep feature.

**Requirements:** R2, R3, R4

**Dependencies:** Units 1-3

**Files:**
- Modify: `src/main.rs`
- Modify: `README.md`
- Test: `tests/integration.rs`

**Approach:**
- Reuse the same saved/protected/regular ordering from app-layer data when formatting preview lines.
- Insert blank lines between sections inside each cleanup group so preview output matches the interactive structure.
- Add a non-color cue for saved rows in preview output when color is unavailable or undesirable.
- Update README/help text so users understand:
  - keep labels persist per repo
  - the storage is local-only
  - `s` toggles keep in the interactive workflow
  - saved rows are excluded from bulk deletion until unsaved

**Patterns to follow:**
- Preview/TUI parity guidance in `docs/solutions/best-practices/mirror-interactive-cleanup-previews-2026-04-12.md`
- Existing `format_preview_lines()` structure in `src/main.rs`

**Test scenarios:**
- Happy path: `--dry-run` shows protected rows first, then saved rows, then regular rows with blank-line segmentation.
- Happy path: saved rows in preview carry a readable saved indicator without losing the branch/secondary/age columns.
- Edge case: a group containing only saved rows still renders cleanly and does not print misleading delete counts.
- Integration: a persisted keep label created in one run changes later `--dry-run` ordering without entering the TUI.
- Test expectation: none -- README/help copy updates are documentation-only, but they should be reviewed against the final interactive behavior.

**Verification:**
- Users can trust preview output as a rehearsal for the new interactive saved-section workflow.

## System-Wide Impact

- **Interaction graph:** `scan_selected_modes()` must incorporate keep-label loading before `CleanupGroup` reaches `App::from_group()`, and both `src/ui.rs` and `format_preview_lines()` in `src/main.rs` must consume the same ordering.
- **Error propagation:** keep-store read/write failures should surface before destructive review proceeds, because missing saved labels change branch eligibility presentation.
- **State lifecycle risks:** persisted labels can outlive the branch that created them; the implementation should ensure stale entries do not render or silently reattach to unrelated data.
- **API surface parity:** interactive and preview modes both need the new section ordering, saved-row indicators, and initial/default behavior semantics.
- **Integration coverage:** tests need to verify persisted state across separate invocations, not just within a single in-memory app instance.
- **Unchanged invariants:** cleanup mode definitions (`gone`, `unpushed`, `closed`), protection semantics, and per-group confirmation remain intact.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Saved-state persistence becomes invisible in preview mode, causing trust drift | Reuse app-layer ordering and add preview-specific saved cues in the same shared branch model |
| Corrupted keep-label store would cause hidden branches to reappear as regular candidates | Fail closed with an actionable error and do not continue cleanup review |
| Worktree clones or shared git dirs make a hardcoded `.git/` path incorrect | Resolve storage through git common-dir APIs instead of filesystem assumptions |
| Saved branches remain eligible for bulk delete through existing shortcuts | Restrict `delete all` and delete toggles to regular rows only |

## Documentation / Operational Notes

- Update `README.md` and `git-broom --help` text to document persistent keep labels and the `s` key.
- Add a short note in `AGENTS.md` or a follow-up learning only if the persisted keep-store pattern becomes reusable beyond this feature.

## Sources & References

- Related code: `src/app.rs`
- Related code: `src/main.rs`
- Related code: `src/ui.rs`
- Test surface: `tests/integration.rs`
- Institutional learning: `docs/solutions/best-practices/mirror-interactive-cleanup-previews-2026-04-12.md`
- Historical context: `docs/archive/2026-04-10-feat-git-broom-interactive-branch-cleanup-plan.md`
- Historical context: `docs/archive/2026-04-10-feat-git-broom-unpushed-mode-plan.md`
