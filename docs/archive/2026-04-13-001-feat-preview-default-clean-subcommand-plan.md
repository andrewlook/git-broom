---
title: feat: Make preview inventory the default and move cleanup behind a subcommand
type: feat
status: archived
date: 2026-04-13
---

# feat: Make preview inventory the default and move cleanup behind a subcommand

## Overview

Change `git-broom` so the default command shows the grouped branch inventory instead of entering destructive review immediately, and move the deletion workflow behind an explicit `clean` subcommand. Add a cached GitHub PR metadata layer so the default inventory remains fast across repeated runs while still showing `closed` / `merged` grouping, PR URLs, and persisted keep labels.

## Problem Frame

The current zero-argument behavior enters the cleanup workflow immediately and refreshes GitHub PR state whenever `closed` mode is in scope. That is useful when the user is already committed to deleting branches, but it is not the main browse path the user described. In normal usage, they want an enriched version of `git branch --sort=-committerdate`: a quick, grouped branch inventory with keep labels, protection markers, and PR links, without paying the GitHub lookup cost every time or being dropped directly into a destructive flow.

The plan needs to preserve the trust model established by the existing grouped previews and cleanup TUI:

- the default output must remain human-readable and grouped by cleanup mode
- the destructive flow must require an explicit command boundary
- cached GitHub data must speed up browsing without making destructive cleanup rely on stale metadata
- keep-label ordering and preview parity must survive the command-surface change

## Requirements Trace

- R1. `git-broom` with no subcommand shows the grouped non-destructive inventory by default instead of entering cleanup review.
- R2. Destructive cleanup is moved behind an explicit subcommand, with `git-broom clean ...` replacing the current zero-arg destructive entrypoint.
- R3. The default inventory remains grouped by cleanup mode and preserves saved / protected / regular ordering, PR URLs when known, and current descriptive headers.
- R4. GitHub PR metadata for `closed` mode is cached per repo so repeated inventory runs usually avoid slow `gh` calls.
- R5. The cache is stored outside tracked files and invalidated or refreshed safely enough that destructive cleanup does not proceed using stale or corrupt PR metadata.
- R6. `clean` refreshes authoritative GitHub data before reviewing or deleting `closed` / `merged` branches.
- R7. The CLI remains understandable and documented, including compatibility behavior for existing `--dry-run` / `--batch` users.

## Scope Boundaries

- No new cleanup modes are added in this pass.
- No background daemon, filesystem watcher, or async prefetch process is added.
- No cross-clone or global PR cache is added; caching remains per repo clone via git metadata.
- No machine-oriented output format is added; the default inventory remains human-readable.
- No new standalone cache-management subcommand is required in this pass.

## Context & Research

### Relevant Code and Patterns

- `src/main.rs` currently owns CLI parsing, top-level dispatch, preview rendering, and destructive execution flow. It is the right place to introduce the preview-vs-clean command split.
- `src/app.rs` already centralizes branch inventory scanning and grouped cleanup construction through `scan_selected_modes()` / `scan_selected_modes_with_progress()`. That shared model should remain the single source of truth for both inventory and cleanup.
- `src/keep_store.rs` already demonstrates the repo’s preferred persistence pattern: a versioned JSON file stored under `git rev-parse --git-common-dir`, outside tracked files.
- `tests/integration.rs` already uses temp repos and fake `gh` binaries to verify closed-mode grouping and keep-store persistence. That existing test harness can validate cache hits, stale cache refreshes, and preview-vs-clean behavior.
- `src/main.rs` still uses manual `std::env::args()` parsing. The current parser is small enough that this CLI contract change does not need to be coupled to a parser-library migration.

### Institutional Learnings

- `docs/solutions/best-practices/mirror-interactive-cleanup-previews-2026-04-12.md` says the preview and destructive review should share one workflow model instead of diverging into separate abstractions. This change should keep that guardrail while making preview the default surface.

### External References

- External research skipped. The repo already has the local patterns needed for git-metadata persistence, grouped preview rendering, and fake-`gh` integration testing.

## Key Technical Decisions

- **Make preview inventory the root command and use `clean` as the destructive subcommand.**
  Rationale: the user’s primary path is browsing with metadata, not immediate deletion. `clean` is an explicit destructive verb that makes the safety boundary clearer than the current zero-arg behavior.

- **Keep one shared grouped inventory model and change only the top-level command routing.**
  Rationale: `CleanupGroup`, branch ordering, and preview formatting already align well. Reusing them avoids drift between the default inventory and the `clean` subcommand.

- **Add a dedicated PR cache store under the git common dir, alongside the keep-label store.**
  Rationale: `src/keep_store.rs` already established a safe, portable repo-local persistence pattern. A sibling cache such as `<git-common-dir>/git-broom/pr-cache.json` keeps this metadata out of tracked files and works across worktrees in the same clone.

- **Cache GitHub PR data for inventory browsing, but force a refresh for `clean` when `closed` mode is involved.**
  Rationale: browsing benefits from speed; destructive cleanup needs fresh authority. This split lets the default command feel fast without weakening deletion safety.

- **Treat `--dry-run` and `--batch` as compatibility aliases for the new default inventory output instead of maintaining distinct modes.**
  Rationale: the user no longer wants separate preview modes as the primary product concept, but compatibility matters. Keeping them as no-op aliases prevents unnecessary breakage while simplifying behavior.

- **Gracefully degrade preview mode when cached `closed` metadata is unavailable and `gh` cannot refresh it; do not degrade the destructive `clean` path.**
  Rationale: the root inventory should still be useful for `gone` / `unpushed` even when GitHub auth is missing. Destructive cleanup should fail closed rather than proceeding with uncertain `closed` classification.

- **Preserve descending committer-date ordering within each section and group.**
  Rationale: the user explicitly compares this tool to `git branch --sort=-committerdate`; the grouped output should remain familiar even as it adds metadata and sectioning.

## Open Questions

### Resolved During Planning

- **What should the destructive subcommand be called?**
  Use `clean`. It is short, explicit, and aligned with the product’s branch-cleanup purpose.

- **Should this change also migrate the CLI to `clap`?**
  No. The parser change is not the core problem, and coupling it to this behavior shift would expand scope without clear user value.

- **Should cached GitHub data be trusted for destructive cleanup?**
  No. `clean` should refresh authoritative data for `closed` mode before review or deletion.

### Deferred to Implementation

- **What cache freshness threshold gives the best UX for inventory mode?**
  The implementation should pick a concrete TTL after seeing how expensive the current `gh` query pattern remains in practice, but it should be short enough that repeated same-session inventory runs stay fast and long enough to avoid refetching on every invocation.

- **How prominently should cache age be displayed in preview output, if at all?**
  This is presentation detail. The implementation can decide whether cache age belongs in a header suffix, a status note, or only in stale-data warnings.

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for review, not implementation specification. The implementing agent should treat it as context, not code to reproduce.*

| Invocation | Purpose | GitHub data policy | Destructive? |
|---|---|---|---|
| `git-broom [modes...]` | Default grouped inventory | Use fresh-enough cache when available; refresh lazily on miss/stale; if refresh is impossible, degrade gracefully for `closed` inventory only | No |
| `git-broom clean [modes...]` | Review + delete selected branches | Refresh `closed` metadata before review when `closed` is selected; write updated cache on success | Yes |
| `git-broom --dry-run` / `git-broom --batch` | Compatibility aliases for default inventory | Same as default inventory | No |

Inventory and cleanup should still share one grouped branch model:

1. Parse CLI into an `Inventory` or `Clean` command intent.
2. Resolve scan options from that intent:
   - preview intent: cache-friendly
   - clean intent: refresh-first for `closed`
3. Build `CleanupGroup` values once in `src/app.rs`.
4. Render those groups either:
   - directly to stdout for the default inventory, or
   - step-by-step through the existing TUI / execution flow for `clean`

## Implementation Units

- [ ] **Unit 1: Reshape the CLI around preview-first inventory and a `clean` subcommand**

**Goal:** Make grouped preview output the default command behavior and move destructive review behind an explicit subcommand without introducing parser sprawl.

**Requirements:** R1, R2, R7

**Dependencies:** None

**Files:**
- Modify: `src/main.rs`
- Modify: `README.md`
- Test: `tests/integration.rs`
- Test: `src/main.rs`

**Approach:**
- Replace the current `OutputMode`-first contract with a top-level command intent such as `Inventory` vs `Clean`.
- Keep positional cleanup-mode filtering (`gone`, `unpushed`, `closed`) available in both entrypoints.
- Treat `--dry-run` and `--batch` as preview aliases for compatibility rather than maintaining separate preview behavior.
- Reject contradictory combinations such as `git-broom clean --dry-run` instead of silently normalizing them.
- Update help text and examples so the default root command is clearly inventory-first and `clean` is clearly destructive.

**Patterns to follow:**
- Existing manual CLI parsing and usage text in `src/main.rs`
- Existing grouped preview rendering in `format_preview_lines()` in `src/main.rs`

**Test scenarios:**
- Happy path: `git-broom` parses to the non-destructive inventory path with all implemented modes selected.
- Happy path: `git-broom clean` parses to the destructive review path with all implemented modes selected.
- Happy path: `git-broom gone` previews only gone branches, while `git-broom clean gone` enters the destructive gone-only flow.
- Edge case: `git-broom --dry-run` and `git-broom --batch` produce the same grouped inventory output as `git-broom`.
- Error path: `git-broom clean --dry-run` is rejected with an actionable CLI error instead of behaving ambiguously.

**Verification:**
- An implementer can run `git-broom` and see grouped inventory output without entering the TUI, while `git-broom clean` still reaches the deletion workflow.

- [ ] **Unit 2: Add repo-local GitHub PR cache persistence**

**Goal:** Persist closed-mode PR metadata in a repo-local cache so inventory runs can reuse recent GitHub state.

**Requirements:** R4, R5

**Dependencies:** Unit 1

**Files:**
- Create: `src/pr_cache.rs`
- Modify: `src/lib.rs`
- Modify: `src/app.rs`
- Test: `tests/integration.rs`

**Approach:**
- Introduce a versioned JSON cache store parallel to `KeepStore`, stored under the git common dir.
- Persist enough metadata to safely reuse cached `closed` / `merged` classification inputs:
  - remote name
  - remote URL or another remote identity token
  - refresh timestamp
  - PR records keyed by head branch
- Define cache freshness in app-layer scan options rather than hardcoding that policy into the storage module.
- Treat missing cache as empty state.
- Treat malformed cache as “refresh required” rather than silently trusting it.
- Remove or rewrite stale cache entries when a fresh GitHub fetch completes.

**Patterns to follow:**
- Versioned git-common-dir storage pattern in `src/keep_store.rs`
- Closed-mode PR record handling in `src/app.rs`

**Test scenarios:**
- Happy path: a cold inventory run writes `pr-cache.json` under the git common dir after a successful GitHub refresh.
- Happy path: a warm inventory run with a fresh cache reuses cached PR metadata without invoking the fake `gh` binary again.
- Edge case: cache records are scoped to the configured remote and do not leak across remotes with different names or URLs.
- Error path: malformed cache content is not trusted; the code falls back to refresh logic instead of classifying branches from corrupted data.
- Integration: a cached closed PR still renders its PR URL in the grouped inventory on a later run without hitting GitHub again.

**Verification:**
- Repeated `git-broom` runs in the same repo reuse cached `closed` metadata and avoid repeated GitHub fetches when the cache is still fresh.

- [ ] **Unit 3: Add intent-aware scan behavior for preview vs clean**

**Goal:** Keep one grouped branch model while changing how `closed` metadata is sourced depending on whether the command is browsing or deleting.

**Requirements:** R3, R4, R5, R6

**Dependencies:** Unit 2

**Files:**
- Modify: `src/app.rs`
- Modify: `src/main.rs`
- Test: `tests/integration.rs`
- Test: `src/app.rs`

**Approach:**
- Introduce a scan-policy concept in `src/app.rs` so the same inventory builder can support:
  - preview intent: cache-first, refresh-on-miss-or-stale
  - clean intent: refresh-first for `closed`
- Preserve saved/protected/regular ordering and PR URL display regardless of whether data came from cache or fresh GitHub results.
- Keep preview rendering on the same grouped formatter path so the default inventory still reflects keep labels and protected rows.
- In preview mode, if `closed` data cannot be refreshed and no usable cache exists, keep `gone` / `unpushed` output working and emit an explicit note that `closed` metadata is unavailable.
- In `clean`, fail before destructive review if authoritative `closed` refresh fails.

**Execution note:** Start with characterization coverage around the current scan entrypoint and group output before changing the preview-vs-clean contract.

**Patterns to follow:**
- Shared grouped scan model in `src/app.rs`
- Preview parity guidance in `docs/solutions/best-practices/mirror-interactive-cleanup-previews-2026-04-12.md`
- Existing keep-label ordering logic in `src/app.rs`

**Test scenarios:**
- Happy path: default inventory still shows protected rows first, saved rows second, and regular rows last within each cleanup group.
- Happy path: `git-broom` with a fresh cache shows `closed` / `merged` groups with PR URLs and no GitHub refresh.
- Happy path: `git-broom clean closed` refreshes GitHub data before review and writes the refreshed cache back out.
- Edge case: if preview mode has no usable `closed` cache and GitHub auth is unavailable, `gone` / `unpushed` groups still render and `closed` is reported as unavailable instead of crashing the whole command.
- Error path: if `clean closed` cannot refresh GitHub data, the destructive flow aborts before any delete review or execution begins.
- Integration: a branch saved via the keep store still appears in the saved section of the default inventory after the command-surface change.

**Verification:**
- Inventory and cleanup still agree on group membership and ordering when fed the same underlying branch data, while cleanup uses fresher `closed` metadata before deletion.

- [ ] **Unit 4: Update user-facing docs and compatibility expectations**

**Goal:** Make the new command model legible in help text, README, and repo guidance so users understand preview-first browsing, cache behavior, and explicit cleanup.

**Requirements:** R1, R2, R4, R7

**Dependencies:** Units 1-3

**Files:**
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `src/main.rs`
- Test: `tests/integration.rs`
- Test: `src/main.rs`

**Approach:**
- Rewrite help text and README examples around:
  - `git-broom` for inventory
  - `git-broom clean ...` for deletion
  - compatibility notes for `--dry-run` / `--batch`
  - cache behavior for `closed` metadata
- Update `AGENTS.md` if the repo-level product preference has now changed from “preview flags” to “preview-first default”.
- Keep documentation explicit that cached GitHub data is a browsing optimization, not the source of truth for destructive cleanup.

**Patterns to follow:**
- Existing help-text style in `src/main.rs`
- Existing product-preference notes in `AGENTS.md`
- Existing README tone and command examples in `README.md`

**Test scenarios:**
- Happy path: `git-broom --help` documents preview-first root behavior and the `clean` subcommand.
- Happy path: docs examples cover default inventory, selective mode preview, and explicit cleanup.
- Edge case: compatibility aliases are documented clearly enough that existing users understand why `--dry-run` no longer changes behavior.
- Integration: README and help text match the actual CLI surface after the parser changes.

**Verification:**
- A new user reading `--help` or `README.md` can understand how to browse branches quickly and how to enter the destructive cleanup flow explicitly.

## System-Wide Impact

- **Interaction graph:** `src/main.rs` command parsing and dispatch will now choose between preview and cleanup intents, while `src/app.rs` must accept scan policy input without forking into separate inventory models. The new PR cache store will sit beside the keep store under the git common dir.
- **Error propagation:** preview mode should surface `closed`-metadata unavailability as a scoped warning or note; `clean` should fail closed before destructive review when authoritative refresh fails.
- **State lifecycle risks:** stale or corrupt PR cache data can misclassify `closed` branches if trusted too broadly. Cache freshness, remote identity checks, and clean-path refreshes need to protect against this.
- **API surface parity:** root inventory output, `--dry-run`, and `--batch` compatibility behavior should all render the same grouped view. `clean` should still reuse the same group ordering and labels before deletion.
- **Integration coverage:** tests need to prove warm-cache reuse, stale-cache refresh, cache-miss fallback behavior, and clean-path refresh semantics through temp repos and fake `gh`.
- **Unchanged invariants:** keep-label persistence, protected-branch handling, section ordering, PR URL display, and remote-first deletion for `closed` cleanup remain intact.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Users rely on the current zero-arg destructive flow | Make `clean` explicit in docs/help, keep preview aliases for compatibility, and ensure the root command clearly reads as non-destructive |
| Cached GitHub state becomes stale enough to mislead the inventory view | Add freshness policy, remote identity scoping, and force fresh data in `clean` |
| GitHub auth is missing and default inventory becomes less useful | Gracefully degrade only the `closed` portion of inventory instead of failing the whole command |
| CLI parsing becomes harder to reason about with command intents plus compatibility flags | Keep manual parsing small, test parser behavior directly, and avoid folding in an unrelated `clap` migration |

## Documentation / Operational Notes

- Update examples in `README.md` and `git-broom --help` so they present inventory-first behavior as the default mental model.
- Document that GitHub PR metadata is cached locally under git metadata and may be refreshed automatically for browsing or forcibly for `clean`.
- If the implementation adds an inline freshness note for `closed` metadata, keep it concise and scoped to inventory mode.

## Sources & References

- Related code: `src/main.rs`
- Related code: `src/app.rs`
- Related code: `src/keep_store.rs`
- Related tests: `tests/integration.rs`
- Institutional learning: `docs/solutions/best-practices/mirror-interactive-cleanup-previews-2026-04-12.md`
- Historical context: `docs/archive/2026-04-10-feat-git-broom-unpushed-mode-plan.md`
