---
title: Customizing cargo-dist release workflow to skip PRs
date: 2026-04-15
category: workflow-issues
module: ci/release
problem_type: workflow_issue
component: tooling
severity: low
applies_when:
  - Using cargo-dist to generate release CI workflows
  - Release workflow runs unnecessarily on feature branch PRs
  - dist plan fails on PRs because no release tag is present
tags: [cargo-dist, github-actions, release, ci, pull-request-trigger, allow-dirty]
---

# Customizing cargo-dist release workflow to skip PRs

## Context

`cargo-dist` auto-generates `.github/workflows/release.yml` with a `pull_request:` trigger (no branch filter) alongside the tag-based `push:` trigger. This causes the Release workflow to run on **every** PR, including feature branches where `dist plan` will fail because there's no release tag context.

The intention behind the default is a dry-run: catch release config breakage on PRs before you push a release tag. In practice, it's noisy for repos where most PRs have nothing to do with releases.

## Guidance

Remove the `pull_request:` trigger from the `on:` block in `.github/workflows/release.yml`, and add `allow-dirty = ["ci"]` to `Cargo.toml` so `dist` doesn't warn about the hand-edited file.

**release.yml** — remove the trigger:

```yaml
# Before
on:
  pull_request:
  push:
    tags:
      - '**[0-9]+.[0-9]+.[0-9]+*'

# After
on:
  push:
    tags:
      - '**[0-9]+.[0-9]+.[0-9]+*'
```

**Cargo.toml** — suppress the drift warning:

```toml
[workspace.metadata.dist]
allow-dirty = ["ci"]
```

## Why This Matters

Without this change, every PR triggers a Release workflow that will fail at the `dist plan` step (no tag context). This creates red checks on PRs that have nothing to do with releases, causing confusion and noise in CI.

The `allow-dirty` setting is necessary because `cargo-dist` tracks whether its generated files match what `dist init` would produce. Without it, `dist` will warn on every run that the CI file has been hand-edited and suggest running `dist init` (which would overwrite your customization).

## When to Apply

- When setting up `cargo-dist` for a new project
- When the Release workflow is showing failures on unrelated PRs
- After running `dist init` which regenerates the workflow with the `pull_request:` trigger

## Examples

The `dist plan` error on a feature branch PR looks like:

```
help: run 'dist init' to update the file
      ('allow-dirty' in Cargo.toml to ignore out of date contents)
```

This is `dist` detecting that the workflow file doesn't match its expected output. The `allow-dirty = ["ci"]` config tells it to stop checking.

If you still want PR-based release validation (e.g., for repos where release config changes frequently), you can keep the trigger but scope it:

```yaml
on:
  pull_request:
    branches:
      - main
    paths:
      - 'Cargo.toml'
      - '.github/workflows/release.yml'
  push:
    tags:
      - '**[0-9]+.[0-9]+.[0-9]+*'
```

## Related

- [cargo-dist documentation](https://opensource.axo.dev/cargo-dist/)
