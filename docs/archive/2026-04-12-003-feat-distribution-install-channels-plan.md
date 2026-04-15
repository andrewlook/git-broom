---
title: feat: Add installable distribution channels
type: feat
status: completed
date: 2026-04-12
deepened: 2026-04-12
---

# feat: Add installable distribution channels

**Target repos:** `git-broom` (this repo) and a new tap repo such as `andrewlook/homebrew-tap`.

## Overview

Make `git-broom` installable through two user-facing channels:

- `cargo install git-broom` via crates.io
- `brew install andrewlook/homebrew-tap/git-broom` via an upstream Homebrew tap

The plan favors a right-sized first release posture for a small Rust CLI:

- publish the crate directly to crates.io
- maintain a source-building Homebrew formula in an upstream tap
- use semantic version tags and documented release discipline
- defer heavier release automation such as `cargo-dist` until the project actually needs prebuilt installers, broader artifact hosting, or lower-maintenance formula updates

## Problem Frame

`git-broom` currently documents only local source installation with `cargo install --path .` in `README.md`. That is fine for contributors, but it is not a durable distribution story for normal users. A public installation path needs:

- stable versioning
- discoverable install commands
- package metadata that is suitable for registry publication
- a repeatable maintainer workflow for releases
- a Homebrew strategy that does not assume `homebrew/core` eligibility on day one

The repo currently has:

- a single Rust crate in `Cargo.toml`
- no git tags yet
- no release workflow in `.github/workflows/`
- enough package metadata for local builds, but not the fuller discoverability/release metadata that Cargo recommends before publishing

This is primarily a packaging and release-operations feature, not a runtime behavior change inside `src/`.

## Requirements Trace

- R1. Users can install the released tool with `cargo install git-broom` once the first public version is published.
- R2. Users can install the released tool with a one-command Homebrew invocation from an upstream tap.
- R3. The release process is based on stable semantic versions and tagged source tarballs rather than mutable branch builds.
- R4. The crate manifest and README expose enough metadata and installation guidance for discovery and support.
- R5. The initial distribution setup stays proportionate to a small single-binary Rust CLI and does not introduce unnecessary release infrastructure.
- R6. Homebrew packaging remains build-from-source friendly and does not require custom binary hosting on day one.

## Scope Boundaries

- No attempt to land `git-broom` in `homebrew/core` in this pass.
- No prebuilt installer matrix, shell installer script, or standalone website in this pass.
- No `cargo-dist` adoption in this pass unless implementation uncovers a concrete blocker in the simpler tag-plus-tap approach.
- No auto-publish to crates.io on the first release without a human-owned publish step.
- No runtime feature changes to the CLI beyond what is necessary for packaging validation.

## Context & Research

### Relevant Code and Patterns

- `Cargo.toml` already has `name`, `version`, `description`, `repository`, and `license`, which is a workable base for crates.io publishing.
- `README.md` already has a contributor-oriented install section that should be expanded into public installation guidance rather than replaced.
- `.github/workflows/ci.yml` already validates formatting, linting, and tests on Linux and macOS; release validation should extend this posture rather than invent a separate quality bar.
- `docs/archive/2026-04-10-feat-git-broom-interactive-branch-cleanup-plan.md` already recorded distribution as a desired outcome and mentioned `cargo-dist`, but no implementation followed from that earlier note.

### Institutional Learnings

- `docs/solutions/best-practices/mirror-interactive-cleanup-previews-2026-04-12.md` reinforces a repo preference for parity across user-facing surfaces. The same idea applies here: the release docs, Cargo metadata, and package-manager install commands should describe the same supported product, not drift into separate stories for contributors vs users.

### External References

- The Cargo Book says crates.io package names are first-come, first-served, recommends filling out `license`, `description`, `homepage`, `repository`, and `readme`, and suggests adding `keywords` and `categories` before publishing. It also recommends running `cargo publish --dry-run` first and treating releases as tag-backed, changelog-backed events.
- The Cargo manifest reference includes `rust-version`, `documentation`, `keywords`, `categories`, `include`, and `exclude` as relevant package metadata and packaging controls.
- Homebrew’s tap documentation recommends a `homebrew-*` repository name, documents `brew tap-new`, recommends direct installation with `brew install user/repository/formula`, and notes that the default tap workflows can build bottles and upload them to GitHub Releases if left in place.
- Homebrew’s acceptable-formulae guidance makes `homebrew/core` a higher bar, explicitly encourages maintaining your own tap when needed, and allows formulae to let tools like `cargo` download versioned libraries during installation.
- The Formula Cookbook requires a real `test do` block and prefers a basic functionality test over a pure `--help` or `--version` smoke test.

## Key Technical Decisions

- Target crates.io plus an upstream Homebrew tap, not `homebrew/core`, for the initial public distribution.
  Rationale: this repo is early, self-submitted, and has no release history yet. An upstream tap gives immediate installability without depending on `homebrew/core` acceptance criteria or popularity thresholds.

- Use a separate tap repository named `homebrew-tap`.
  Rationale: Homebrew recommends GitHub tap repositories start with `homebrew-`, and direct installation from `user/homebrew-tap/formula` gives users a one-command install path without polluting the main repo with tap-specific workflow state.

- Keep the Homebrew formula source-building in the first pass.
  Rationale: Homebrew explicitly allows formulae to use `cargo` to fetch versioned libraries during installation, and a source-building formula avoids the extra complexity of cross-platform binary production before the project proves it needs bottles or installers.

- Use version tags as the source of truth for both crates.io publishing and Homebrew formula updates.
  Rationale: Cargo and Homebrew both assume stable versioned releases. A tagged source tarball keeps the two channels aligned and gives the formula a checksummed immutable source URL.

- Add manifest metadata and package-boundary checks before the first publish.
  Rationale: the current manifest is minimally sufficient for local builds but is missing discoverability and packaging details such as `rust-version`, `keywords`, and `categories`, and the repo should verify what gets packed before an irreversible publish.

- Start with a documented manual publish step to crates.io, then automate later if it proves painful.
  Rationale: crates.io publishes are permanent. The first release should optimize for clarity and reversibility of process design, not for full automation.

- Defer `cargo-dist` for now.
  Rationale: `cargo-dist` is useful when the repo wants generated release CI, hosted installers, or broader package-manager publishing. For a single binary whose immediate goals are crates.io plus an upstream tap, the added config is not yet justified.

## Open Questions

### Resolved During Planning

- **Should the first Homebrew target be `homebrew/core` or an upstream tap?**
  Use an upstream tap first.

- **Should the first Homebrew formula build from source or rely on prebuilt binaries?**
  Build from source first; treat bottles as an optional follow-on that the tap’s default workflows can handle later.

- **Should this plan adopt `cargo-dist` immediately?**
  No. Keep the initial release process simpler and revisit once the project needs broader distribution automation.

- **Should crates.io publishing be fully automated on day one?**
  No. The first public release should keep a human-owned publish step.

### Deferred to Implementation

- **Is the exact crate name `git-broom` still available at publish time?**
  Verify immediately before first publish and choose a fallback name only if needed.

- **What exact `rust-version` should be declared in `Cargo.toml`?**
  Set it to the minimum version the maintainer is willing to support once release validation confirms the baseline.

- **Should the Homebrew formula declare `git` as an explicit runtime dependency, or only document it as a runtime prerequisite?**
  Decide during implementation based on how Homebrew handles system `git` on the supported platforms and how strict the formula test needs to be.

- **Should the tap keep the default bottle workflows enabled immediately, or ship the source formula first and enable bottles in a second pass?**
  Either is acceptable; choose based on the maintenance appetite for the new tap.

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for review, not implementation specification. The implementing agent should treat it as context, not code to reproduce.*

| Channel | User command | Source of truth | Repo owner surface | Initial artifact strategy |
|---|---|---|---|---|
| Cargo | `cargo install git-broom` | crates.io release version | `git-broom` | published crate |
| Homebrew | `brew install andrewlook/homebrew-tap/git-broom` | git tag tarball + formula SHA | `homebrew-tap` | source-build formula |

Release shape:

1. Prepare publishable crate metadata and docs in `git-broom`.
2. Tag a stable semver release in `git-broom`.
3. Run release validation and perform a human-owned `cargo publish`.
4. Update `homebrew-tap/Formula/git-broom.rb` to the matching tagged tarball and checksum.
5. Users install from either crates.io or the tap with consistent versioned expectations.

## Alternative Approaches Considered

- **`homebrew/core` first**
  Rejected for the first pass because it adds review/eligibility uncertainty before the project has any release history.

- **Adopt `cargo-dist` immediately**
  Rejected for the first pass because the project does not yet need generated installers or multi-platform release hosting to satisfy the requested install commands.

- **Keep Homebrew formula in this repo**
  Rejected because taps are expected to be Git repositories with Homebrew-specific structure and maintenance, and a separate tap keeps release/package concerns cleaner.

## Implementation Units

- [ ] **Unit 1: Prepare the crate for registry publication**

**Goal:** Make the main crate manifest and public docs ready for crates.io publication and public discovery.

**Requirements:** R1, R3, R4, R5

**Dependencies:** None

**Files:**
- Modify: `Cargo.toml`
- Modify: `README.md`
- Create: `CHANGELOG.md`

**Approach:**
- Add the package metadata that is still missing or too thin for public distribution, including:
  - `rust-version`
  - `keywords`
  - `categories`
  - any non-redundant `documentation` or `homepage` values if you choose to expose them
- Expand `README.md` from contributor-only install guidance into public install guidance with:
  - `cargo install git-broom`
  - Homebrew tap install command
  - runtime requirements such as `git` and optional `gh`
- Add a human-curated `CHANGELOG.md` to give releases a stable notes surface.
- Check the packaged crate boundary and explicitly add `include` / `exclude` rules only if `cargo package --list` shows unnecessary files.

**Patterns to follow:**
- Existing concise user-facing style in `README.md`
- Cargo metadata recommendations from the Cargo Book and manifest reference

**Test scenarios:**
- Test expectation: none -- metadata and documentation preparation unit.

**Verification:**
- The crate manifest is publish-ready, the README advertises the two supported install channels, and the release notes surface exists before the first publish.

- [ ] **Unit 2: Add release validation and maintainer workflow in the main repo**

**Goal:** Give the main repo a repeatable release path that validates packaging and versioned releases before a permanent publish.

**Requirements:** R1, R3, R4, R5

**Dependencies:** Unit 1

**Files:**
- Create: `.github/workflows/release.yml`
- Create: `docs/releasing.md`
- Modify: `README.md`

**Approach:**
- Add a release-oriented GitHub Actions workflow that runs on version tags or manual dispatch and verifies:
  - existing Rust quality checks still pass
  - packaging succeeds through `cargo publish --dry-run`
  - the packaged crate contents are reviewable before publishing
- Document the maintainer workflow in `docs/releasing.md`, including:
  - version bump expectations
  - changelog update
  - tag creation
  - human-owned crates.io publish
  - follow-up Homebrew tap update
- Keep the first implementation manual at the irreversible step (`cargo publish`) rather than hiding it behind tag automation immediately.

**Patterns to follow:**
- Existing GitHub Actions style and quality bar in `.github/workflows/ci.yml`
- Existing documentation-first operational guidance in `README.md`

**Test scenarios:**
- Test expectation: none -- workflow and release-process configuration unit.

**Verification:**
- Maintainers can validate a release candidate without publishing it, and the repo has one written source of truth for how version tags map to public releases.

- [ ] **Unit 3: Create the upstream Homebrew tap and formula**

**Goal:** Make `git-broom` installable from Homebrew with a one-command upstream tap install.

**Requirements:** R2, R3, R5, R6

**Dependencies:** Units 1-2

**Files:**
- Create: `Formula/git-broom.rb` (in `homebrew-tap`)
- Create: `README.md` (in `homebrew-tap`)

**Approach:**
- Create a new tap repository using Homebrew’s standard tap layout.
- Keep the formula under `Formula/` and point it at a tagged, checksummed source tarball from the main repository.
- Build from source in the formula’s install path rather than introducing separate binary hosting.
- Add a real `test do` block that exercises basic functionality without network access or GitHub auth, preferably by creating a small temp git repo and asserting a simple local command path rather than relying only on `--help`.
- Add tap-specific docs showing both:
  - direct install: `brew install andrewlook/homebrew-tap/git-broom`
  - optional manual tap path when users want it
- Decide whether to keep the default `brew tap-new` GitHub workflows enabled immediately or land them after the source formula works locally.

**Patterns to follow:**
- Homebrew’s recommended `homebrew-*` tap naming and `Formula/` layout
- Homebrew Formula Cookbook guidance for `test do`

**Test scenarios:**
- Happy path: `brew install andrewlook/homebrew-tap/git-broom` builds and links the binary from the tap.
- Happy path: `brew test git-broom` passes using a non-interactive local smoke path in a temporary directory.
- Edge case: formula install uses a tagged, checksummed source tarball rather than a moving branch reference.
- Error path: formula update fails loudly when the tarball URL or SHA no longer matches the released source.
- Integration: the tap README and formula install path match the install command documented in the main repo README.

**Verification:**
- A user on a supported Homebrew platform can install `git-broom` from the tap with one command and the formula has a meaningful automated test.

- [ ] **Unit 4: Align public install surfaces and release handoff**

**Goal:** Keep the crate registry, main repo docs, and tap docs synchronized so distribution does not drift after the first release.

**Requirements:** R1, R2, R3, R4

**Dependencies:** Units 1-3

**Files:**
- Modify: `README.md`
- Modify: `docs/releasing.md`
- Modify: `CHANGELOG.md`
- Modify: `README.md` (in `homebrew-tap`)
- Modify: `Formula/git-broom.rb` (in `homebrew-tap`)

**Approach:**
- Add a release checklist that explicitly couples:
  - crate version bump
  - changelog entry
  - git tag
  - crates.io publish
  - tap formula version/SHA update
- Ensure the main README and tap README never disagree on:
  - current install commands
  - runtime prerequisites
  - support expectations for Cargo vs Homebrew users
- Keep the first release handoff simple enough that a maintainer can perform it deliberately without tool-specific hidden state.

**Patterns to follow:**
- Existing repo preference for concise, human-readable operational docs
- Versioned release expectations from Cargo and Homebrew docs

**Test scenarios:**
- Test expectation: none -- documentation and release-discipline alignment unit.

**Verification:**
- The maintainer can follow one written flow from tag to published crate to updated tap without ad hoc remembered steps, and user-facing install docs stay consistent across repos.

## System-Wide Impact

- **Interaction graph:** This work touches Cargo registry metadata, GitHub tag/release operations, the main repo README, GitHub Actions release validation, and a second Git repository for the Homebrew tap.
- **Error propagation:** Version/tag mistakes will now affect public install commands; the release process must fail before publish when packaging or version alignment is wrong.
- **State lifecycle risks:** crates.io publishes are permanent, while Homebrew formula updates are mutable Git history in the tap repo; the docs need to distinguish those operational realities.
- **API surface parity:** The public install commands become part of the project’s external contract and must stay aligned with actual published package names and tap names.
- **Integration coverage:** The most important integration surface is version parity across `Cargo.toml`, git tags, crates.io releases, and `Formula/git-broom.rb`.
- **Unchanged invariants:** Existing runtime behavior, cleanup modes, and TUI flows remain unchanged; this plan only changes how users acquire the tool.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| The crate name `git-broom` becomes unavailable before first publish | Verify availability immediately before publish and choose a fallback before public docs or tap formula are finalized |
| Homebrew formula drifts from the published crate version | Tie tap updates to the same release checklist and semver tags used for crates.io |
| A source-building formula creates too much install friction | Start with source builds, but leave room to enable tap bottle workflows or adopt heavier automation later |
| Release automation is too thin and maintainers forget a step | Add a repo-owned `docs/releasing.md` checklist before the first public release |
| `homebrew/core` expectations are assumed by users too early | Document the upstream tap as the supported Homebrew channel and defer any core submission discussion |

## Documentation / Operational Notes

- `README.md` should stop framing installation solely as a contributor workflow once public install channels exist.
- The tap repo should include its own `README.md` even if the main repo has install docs; Homebrew users often land in the tap first.
- If the tap later enables bottles or the main repo later adopts `cargo-dist`, update this plan’s release assumptions instead of layering new tooling on top of stale docs.

## Sources & References

- Related code: `Cargo.toml`
- Related code: `README.md`
- Related code: `.github/workflows/ci.yml`
- Related doc: `docs/archive/2026-04-10-feat-git-broom-interactive-branch-cleanup-plan.md`
- Institutional learning: `docs/solutions/best-practices/mirror-interactive-cleanup-previews-2026-04-12.md`
- External docs: Cargo Book publishing guidance
- External docs: Cargo manifest reference
- External docs: Homebrew tap creation and maintenance documentation
- External docs: Homebrew Formula Cookbook
- External docs: Homebrew acceptable-formulae guidance
