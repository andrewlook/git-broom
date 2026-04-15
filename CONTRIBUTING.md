# Contributing

## Requirements

- Rust toolchain with `rustfmt` and `clippy`
- `gh` with an authenticated session (for GitHub-backed groups)

This repo includes `rust-toolchain.toml` so a standard Rust setup installs the right components automatically.

## Running locally

```bash
# preview mode (no deletions)
cargo run

# destructive workflow
cargo run -- clean

# filter groups
cargo run -- -g gone,unpushed
cargo run -- clean -g gone,nopr
```

## Running the tests

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Pre-commit hook

This repo includes a checked-in pre-commit hook at `.githooks/pre-commit`.

Enable it once per clone:

```bash
git config core.hooksPath .githooks
```

What it does:

- runs `rustfmt` on staged Rust files
- automatically stages any formatting changes made by the hook
- runs `cargo clippy --all-targets --all-features -- -D warnings`
- fails the commit if linting fails

## Supply-chain checks

Advisory and dependency-audit tools are recommended locally and enforced in CI. They are **not** part of the pre-commit hook — advisory databases change independently of the code being committed, so keeping the hook fast and deterministic is more important.

### cargo-audit

Checks `Cargo.lock` against the [RustSec Advisory Database](https://rustsec.org) for known vulnerabilities, yanked crates, and unmaintained dependencies.

```bash
cargo install cargo-audit
cargo audit
```

### cargo-vet

Tracks whether third-party dependencies have been audited or are covered by trusted upstream audit sets. Run this after adding or updating dependencies:

```bash
cargo install cargo-vet
cargo vet
```

If `cargo vet` fails after a dependency change, you'll need to either record an audit or add an exemption in `supply-chain/`. See the [cargo-vet book](https://mozilla.github.io/cargo-vet/) for details.

For detailed usage (certifying deps, refreshing imports, CI behavior), see [docs/reference/supply-chain.md](docs/reference/supply-chain.md).

### Devcontainers

Intentionally deferred. This repo has a lightweight Rust toolchain setup (`rust-toolchain.toml`) with no database or service dependencies, so a devcontainer would add maintenance without solving a current problem. Revisit if the repo gains service dependencies, onboarding friction increases, or Codespaces usage becomes common.

## CI

GitHub Actions runs:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo audit` (on PRs, pushes to main, and weekly schedule)
- `cargo vet --locked` (on PRs and pushes to main)

Tests run on both Linux and macOS. Supply-chain checks run in a separate workflow.

## Bumping The Version

When you ship user-facing changes:

1. Pick the next semver version.
2. Update `version` in `Cargo.toml`.
3. Update the root package version entry in `Cargo.lock`.
4. Add a new dated section to `CHANGELOG.md` summarizing the user-visible changes.
5. Run the verification commands above.

For this repo, the changelog and version bump should move together in the same PR.

## Releasing

The maintainer release flow lives in [docs/releasing.md](docs/releasing.md).

Use that doc for:

- the first local `cargo publish`
- the required GitHub secret setup for tag-driven releases
- the semver tag format expected by the release workflow
