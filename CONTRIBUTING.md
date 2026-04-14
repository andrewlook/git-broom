# Contributing

## Local Verification

Run the normal Rust checks before opening or updating a PR:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

The repo also ships a checked-in pre-commit hook in `.githooks/pre-commit`. If this clone is not already configured to use it, run:

```bash
git config core.hooksPath .githooks
```

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
