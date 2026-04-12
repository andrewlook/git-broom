---
title: refactor: Add Rust supply-chain checks
type: refactor
status: active
date: 2026-04-12
deepened: 2026-04-12
---

# refactor: Add Rust supply-chain checks

## Overview

Adopt lightweight Rust supply-chain controls that fit `git-broom`'s current size and maintenance posture:

- add `cargo-audit` as an immediately useful dependency-vulnerability check
- add `cargo-vet` in a staged way so third-party dependency review becomes visible and enforceable without making contributor setup brittle
- keep local and CI checks aligned, but avoid putting networked security checks into the pre-commit hook
- explicitly defer a `.devcontainer/` rollout for now because this repo already has a simple Rust toolchain setup and no service stack

## Problem Frame

`git-broom` already enforces formatting, linting, and tests locally and in GitHub Actions, but it does not currently check dependency advisories or maintain any reviewed-dependency policy. For a Rust CLI, the two relevant additions are:

- `cargo-audit` to detect known vulnerable, yanked, or unmaintained crates from the RustSec ecosystem
- `cargo-vet` to track whether third-party dependencies have been audited locally or by trusted upstream audit sets

This work needs to improve supply-chain visibility without turning a small single-crate CLI into a high-friction security program. The repo also already has a low-overhead contributor experience:

- one crate in `Cargo.toml`
- a pinned `stable` toolchain with `rustfmt` and `clippy` in `rust-toolchain.toml`
- one existing CI workflow in `.github/workflows/ci.yml`

That makes this a policy-and-tooling change, not a broader environment rebuild.

## Requirements Trace

- R1. The repo detects newly disclosed dependency advisories even when no code changes have landed recently.
- R2. Contributors can run the same supply-chain checks locally, but local setup remains optional and separate from the pre-commit hook.
- R3. CI enforces the repo's chosen dependency-safety posture on pull requests in a way that matches the committed policy files.
- R4. Supply-chain automation stays proportionate to a small Rust CLI and does not introduce unnecessary drift from the existing Rust workflow.
- R5. Any adoption of development containers must be justified by repeatable onboarding or CI benefits that the current Rust toolchain setup does not already provide.

## Scope Boundaries

- No replacement of the existing Rust lint/test workflow in `.github/workflows/ci.yml`.
- No addition of networked advisory or audit tools to `.githooks/pre-commit`.
- No attempt to make `cargo-vet` prove perfect review coverage on day one; initial exemptions/imports are acceptable if they are committed and reviewable.
- No `.devcontainer/` or Codespaces rollout in this pass unless research later shows a concrete repo-specific need that outweighs the maintenance cost.
- No SBOM, binary attestation, or release-signing work in this pass.

## Context & Research

### Relevant Code and Patterns

- `.github/workflows/ci.yml` already separates lint and test jobs and is the natural place to mirror additional non-interactive quality checks.
- `README.md` already documents local requirements, local checks, pre-commit behavior, and CI expectations; that is the right contributor-facing surface for local `cargo-audit` / `cargo-vet` guidance.
- `Cargo.toml` shows a single-crate workspace with a small direct dependency set.
- `rust-toolchain.toml` already pins the expected Rust toolchain components, which reduces the need for a full devcontainer just to standardize Rust itself.
- `.github/workflows/ci.yml` already runs tests on both Linux and macOS, so a devcontainer would not replace the need for cross-platform CI coverage.

### Institutional Learnings

- `docs/solutions/best-practices/mirror-interactive-cleanup-previews-2026-04-12.md` reinforces a general repo preference for keeping local and non-interactive automation aligned instead of inventing separate behaviors. The same principle applies here: supply-chain checks should be the same logical checks locally and in CI, even if the trigger surfaces differ.

### External References

- RustSec documents `cargo-audit` as a `Cargo.lock` auditor and points to the `rust-audit-check` GitHub Action for auditing changes and scheduled dependency audits.
- The `cargo-vet` book explicitly supports importing trusted audit sets and recommends a CI configuration step, which fits a committed-policy workflow better than an informal local-only practice.
- GitHub Actions scheduled workflows run from the default branch, which makes them suitable for recurring advisory checks even without fresh commits.
- GitHub dependency graph documentation recommends `Cargo.lock` / `Cargo.toml` for Cargo projects, which makes Dependabot a useful complement to `cargo-audit`.
- The Development Container Specification is aimed at repeatable dev, CI, and test environments, but this repo currently has a lightweight toolchain and no service orchestration burden to justify that extra layer yet.

## Key Technical Decisions

- Use both local and GitHub Actions surfaces for supply-chain checks, but with different purposes.
  Rationale: local commands give contributors fast feedback when they touch dependencies; CI provides enforcement and catches issues that appear after merge. Using only local checks is unenforceable, while using only CI slows the feedback loop.

- Keep `cargo-audit` out of the pre-commit hook.
  Rationale: advisory checks depend on network-fetched databases and change independently of the code being committed. That makes them a poor fit for an always-on hook that should stay fast and deterministic.

- Add `cargo-audit` first and make it the low-friction baseline.
  Rationale: it provides immediate value for known advisories with minimal repo-specific policy maintenance.

- Adopt `cargo-vet` as a committed-policy tool, not a purely local convention.
  Rationale: `cargo-vet` only pays off if the audit/import/exemption files are checked in and CI verifies them. A local-only `cargo-vet` workflow would create invisible policy drift and inconsistent contributor expectations.

- Roll out `cargo-vet` in a staged way.
  Rationale: the repo has roughly one hundred total dependency nodes, so bootstrapping audits/imports is tractable, but enforcing a zero-exemption posture immediately would add avoidable friction. Start with imported audits and explicit exemptions, then ratchet down over time.

- Add Dependabot for Cargo and GitHub Actions now; add `devcontainers` updates only if a devcontainer is introduced later.
  Rationale: advisory scanning and update automation are complementary. Dependabot helps reduce exposure windows, while `cargo-audit` detects issues that already exist in the lockfile.

- Defer `.devcontainer/` adoption for now.
  Rationale: this repo already has a low-friction Rust setup, no database/service stack, and no evidence of environment drift that would justify asking contributors to install Docker and editor/container tooling. The existing CI matrix already covers Ubuntu and macOS, so a devcontainer would standardize only one contributor environment rather than replacing cross-platform verification.

## Open Questions

### Resolved During Planning

- **Should `cargo-audit` and `cargo-vet` be local checks, GitHub Actions checks, or both?**
  Both. Document them for local use, but rely on GitHub Actions for enforcement and scheduled coverage. Do not move them into pre-commit.

- **Should `cargo-audit` run only on dependency-changing pull requests?**
  No. It should run on pull requests and on a recurring schedule, because advisories can appear after `Cargo.lock` is unchanged.

- **Should `cargo-vet` be enforced only locally?**
  No. If adopted, it should be committed and CI-checked with locked inputs so the repo has one auditable dependency-review state.

- **Is a devcontainer worth adopting now?**
  Not for this repo as it exists on 2026-04-12. Revisit only if onboarding pain, Codespaces usage, or additional native/service dependencies emerge.

### Deferred to Implementation

- **Should `cargo-audit` live inside `.github/workflows/ci.yml` or in a separate workflow such as `.github/workflows/supply-chain.yml`?**
  Either is acceptable. The implementation should choose the layout that keeps scheduled triggers and dependency-specific path filters easiest to read.

- **Which trusted audit sets should `cargo-vet` import initially?**
  Choose the smallest credible set that materially reduces exemptions for this dependency tree. The exact imports should be confirmed during implementation.

- **Should `cargo-vet` be required immediately on every pull request, or land first in a soft-launch branch and become required after the initial policy files settle?**
  Decide during implementation based on how noisy the initial exemption/import set is.

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for review, not implementation specification. The implementing agent should treat it as context, not code to reproduce.*

| Surface | `cargo-audit` | `cargo-vet` | Devcontainer |
|---|---|---|---|
| Local contributor workflow | documented optional command | documented command for dependency changes and audit-set refresh | deferred |
| Pre-commit hook | no | no | n/a |
| Pull request CI | yes | yes, once policy files are committed | no |
| Scheduled CI on default branch | yes | optional; only if maintainers want automated drift detection beyond PR enforcement | no |
| Committed repo artifacts | none beyond workflow/docs | `supply-chain/` policy files | deferred |

Recommended rollout shape:

1. Add contributor docs plus automated `cargo-audit`.
2. Add Dependabot for Cargo and GitHub Actions.
3. Bootstrap `cargo-vet` with committed policy files and locked CI verification.
4. Revisit devcontainers only if future repo complexity changes the cost/benefit tradeoff.

## Implementation Units

- [ ] **Unit 1: Document the local supply-chain workflow**

**Goal:** Add clear contributor guidance for running dependency security checks locally without expanding the pre-commit hook.

**Requirements:** R2, R4, R5

**Dependencies:** None

**Files:**
- Modify: `README.md`

**Approach:**
- Add a short "Supply-chain checks" subsection near the existing local checks / CI documentation.
- Document local installation and usage expectations for `cargo-audit`.
- Document when contributors are expected to use `cargo-vet` locally, especially after dependency changes.
- Add a brief note that `.devcontainer/` is intentionally deferred for now, plus the concrete triggers that would justify revisiting it later.
- Make the separation explicit:
  - format/lint/test remain the baseline local checks
  - advisory/audit tooling is recommended locally and enforced in CI
  - pre-commit stays fast and deterministic

**Patterns to follow:**
- Existing contributor guidance structure in `README.md`
- Existing repo preference for concise, human-readable operational docs

**Test scenarios:**
- Test expectation: none -- documentation-only unit.

**Verification:**
- A contributor can read `README.md` and understand which commands are optional local checks, which checks CI enforces, and why supply-chain tools are not part of pre-commit.

- [ ] **Unit 2: Add automated dependency advisory coverage**

**Goal:** Add GitHub automation that catches known dependency advisories promptly and keeps update automation aligned with Cargo usage in the repo.

**Requirements:** R1, R2, R3, R4

**Dependencies:** Unit 1

**Files:**
- Create: `.github/workflows/supply-chain.yml`
- Create: `.github/dependabot.yml`
- Modify: `README.md`

**Approach:**
- Add a dedicated supply-chain workflow or equivalent CI job that runs `cargo-audit`:
  - on pull requests
  - on pushes to `main`
  - on a weekly schedule for default-branch advisory discovery
- Scope path filters so dependency-related edits trigger the workflow when appropriate, while keeping the scheduled run unconditional.
- Add Dependabot updates for:
  - `cargo`
  - `github-actions`
  - `devcontainers` only if a devcontainer is later added
- Keep the workflow independent from the main lint/test jobs so failures are attributable and policy changes stay easy to review.

**Patterns to follow:**
- Existing GitHub Actions style in `.github/workflows/ci.yml`
- Existing README sections for local checks, hooks, and CI behavior

**Test scenarios:**
- Test expectation: none -- workflow and dependency-management configuration only.

**Verification:**
- The repo has automated advisory detection on pull requests and on a weekly default-branch schedule, and Dependabot is configured for the package ecosystems the repo actually uses.

- [ ] **Unit 3: Bootstrap `cargo-vet` as a committed policy**

**Goal:** Introduce a reviewable `cargo-vet` policy that records trusted imports, exemptions, and future audits in version control.

**Requirements:** R2, R3, R4

**Dependencies:** Units 1-2

**Files:**
- Create: `supply-chain/config.toml`
- Create: `supply-chain/audits.toml`
- Create: `supply-chain/imports.lock`
- Modify: `.github/workflows/supply-chain.yml`
- Modify: `README.md`

**Approach:**
- Initialize `cargo-vet` and commit its generated policy files.
- Import one or more trusted audit sets so the initial exemption set stays reviewable and reasonably small.
- Treat the committed `supply-chain/` directory as the source of truth for dependency-review policy.
- Add CI verification using locked inputs so pull requests must update policy files when dependency changes require it.
- Document the maintainer workflow for refreshing imports or recording local audits without over-specifying exact command choreography in the plan.

**Execution note:** Start with a pragmatic bootstrap. The first pass should favor explicit committed exemptions over aspirational zero-exemption policy.

**Patterns to follow:**
- Existing repo convention of keeping enforcement visible in version-controlled config
- Existing CI split between human-readable README guidance and mechanical GitHub workflow enforcement

**Test scenarios:**
- Test expectation: none -- policy/bootstrap configuration only.

**Verification:**
- A dependency-changing pull request cannot merge without either satisfying the existing `cargo-vet` policy or explicitly updating the committed audit/import/exemption files.

## System-Wide Impact

- **Interaction graph:** This work touches contributor docs, GitHub Actions policy, dependency-update automation, and committed supply-chain metadata, but does not affect runtime CLI behavior in `src/`.
- **Error propagation:** CI failures will shift some dependency risk from post-merge discovery to pull-request time; contributors need readable failure surfaces so policy failures are actionable.
- **State lifecycle risks:** `cargo-vet` introduces long-lived policy files under `supply-chain/`; these become part of normal dependency-change reviews and must not be treated as generated noise.
- **API surface parity:** If a devcontainer is ever added later, Dependabot configuration and contributor docs must be updated in the same pass.
- **Integration coverage:** The key integration surface is between local contributor expectations, scheduled workflow behavior, and committed policy files. Reviewers should verify these stay aligned.
- **Unchanged invariants:** Existing Rust formatting, lint, and test commands remain the primary baseline checks and are not replaced by this plan.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| `cargo-audit` introduces noisy failures from newly published advisories | Keep it as a dedicated workflow with readable failure output and a scheduled run so maintainers see issues promptly and can triage deliberately |
| `cargo-vet` adds contributor friction when dependencies change | Stage adoption, import trusted audit sets, and document the local maintainer workflow clearly |
| Dependabot or workflow churn creates too much maintenance for a small repo | Limit updates to ecosystems actually used by the repo and review the noise level after initial rollout |
| A devcontainer would add maintenance without solving a current problem | Defer adoption and record explicit re-evaluation triggers instead of building speculative environment infrastructure |

## Documentation / Operational Notes

- Update `README.md` so the documented CI story matches the actual workflows committed under `.github/workflows/`.
- If `cargo-vet` becomes required, reviewers should treat `supply-chain/` diffs as substantive policy changes, not mechanical churn.
- If the repo later adds `.devcontainer/`, the implementation should revisit this plan, update Dependabot ecosystems, and re-evaluate whether CI should reuse the same container definition.

## Sources & References

- Related code: `.github/workflows/ci.yml`
- Related code: `Cargo.toml`
- Related code: `rust-toolchain.toml`
- Institutional learning: `docs/solutions/best-practices/mirror-interactive-cleanup-previews-2026-04-12.md`
- External docs: RustSec `cargo-audit` and `rust-audit-check`
- External docs: `cargo-vet` book (`Introduction`, `Importing Audits`, `Configuring CI`)
- External docs: GitHub Actions scheduled workflow documentation
- External docs: GitHub dependency graph / Dependabot ecosystem documentation
- External docs: Development Container Specification overview
