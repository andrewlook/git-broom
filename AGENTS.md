# git-broom Agent Notes

## Repo Shape

- `src/main.rs`: CLI parsing, output-mode dispatch, terminal lifecycle
- `src/app.rs`: branch scanning, cleanup-group construction, protections, delete decisions
- `src/ui.rs`: ratatui rendering for the interactive review flow
- `tests/integration.rs`: temp-repo integration coverage
- `docs/plans/`: active plans
- `docs/archive/`: completed or superseded plans
- `docs/solutions/`: documented learnings and best practices

## Knowledge Store

`docs/solutions/` contains documented solutions and preferences for this repo, organized by category with YAML frontmatter fields such as `module`, `problem_type`, `component`, and `tags`.

Relevant when implementing or debugging in an area that already has a documented learning. Start with filename search, then frontmatter or content search.

Current learnings:

- `docs/solutions/best-practices/mirror-interactive-cleanup-previews-2026-04-12.md`

## Product Preferences

- Favor human-readable output over script-oriented output unless machine-readability is an explicit requirement.
- Treat `--dry-run` and `--batch` as previews of the interactive workflow, not separate reporting surfaces.
- Keep destructive flows step-based when there are multiple cleanup modes. Review one group at a time and confirm per group.
- Keep protected branches visible in previews and interactive review. Label them inline instead of silently filtering them out.
- When a protected branch is selected for deletion, explain why it is ineligible instead of ignoring the action.
- Persistent saved/keep labels live under the git common dir at `git-broom/keep-labels.json`, not in tracked files.
- Preserve parity between interactive and non-interactive modes:
  - same group order
  - same mode/explainer text
  - same branch visibility rules
  - similar row structure where practical
  - same protected / saved / regular ordering

## Working Conventions

- For Rust changes, run:
  - `cargo fmt --all`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
- When embedding shell scripts in Rust tests or helpers, prefer raw multiline string literals with real line breaks over escaped inline `\n` strings.
- The repo has a checked-in pre-commit hook at `.githooks/pre-commit`; formatting changes are auto-staged and clippy failures block commits.
- When a plan is implemented, move it from `docs/plans/` to `docs/archive/`.
