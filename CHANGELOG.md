# Changelog

All notable changes to `git-broom` should be documented in this file.

## [0.1.1] - 2026-04-13

### Added

- Preview-first CLI flow where `git-broom` shows grouped branch inventory by default and destructive actions move to `git-broom clean`.
- GitHub-backed branch groups for `pr`, `nopr`, `closed`, and `merged`, plus repo-local PR metadata caching for preview.
- Persistent per-repo keep labels stored under the git common dir.
- In-TUI review and execution flow, including progressive command execution, fail-fast behavior, and in-window failure details.

### Changed

- Preview and triage layouts now use the same compact row shape, with short ages and PR URLs prioritized over less important text.
- Non-PR commit subjects are capped to avoid unnecessary branch-name truncation.
- Cleanup execution now stays in the TUI and shows command progress, including a spinner for the active command.
