# Releasing

## First crates.io release

Before the first publish:

1. Make sure the crate name is still available on crates.io.
2. Draft the next changelog section from merged PRs since the last tag:

```bash
./scripts/draft-release-changelog.sh 0.1.2
```

3. Curate that output into `CHANGELOG.md`, then confirm `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md` are aligned to the version you want to publish.
4. Run the normal verification commands:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

5. Inspect the packaged crate contents:

```bash
cargo package --list
```

6. Validate the publish without uploading:

```bash
cargo publish --dry-run --locked
```

7. Publish from your local machine:

```bash
cargo login
cargo publish --locked
```

8. Tag the released commit:

```bash
git tag v0.1.1
git push origin v0.1.1
```

If the matching version is already on crates.io, the release workflow will skip the publish step and still create the GitHub release.

## Tag-driven releases after setup

After the first local publish, you can let GitHub Actions handle future releases from tags.

### One-time GitHub setup

Add this repository secret:

- `CARGO_REGISTRY_TOKEN`: crates.io API token with publish access

### Release flow

1. Update `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`.
2. A helper script can draft the PR list for that changelog section:

```bash
./scripts/draft-release-changelog.sh 0.1.2
```

3. Merge the release commit to `main`.
4. Create and push a semver tag that matches the manifest version:

```bash
git tag v0.1.2
git push origin v0.1.2
```

The release workflow will then:

- verify the tag matches `Cargo.toml`
- run formatting, clippy, and tests
- run `cargo publish --dry-run --locked`
- package the crate
- publish to crates.io when that version is not already published
- create a GitHub release using the matching `CHANGELOG.md` section

If the workflow fails before publish, fix the problem and push a new release commit plus a new tag.
