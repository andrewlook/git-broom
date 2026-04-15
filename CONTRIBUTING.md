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

## CI

GitHub Actions runs:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`

Tests run on both Linux and macOS.

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
