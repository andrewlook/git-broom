# git-broom

`git-broom` is a small Rust CLI for cleaning up stale local git branches after squash-merge workflows leave tracking branches behind.

Current v1 scope is intentionally narrow:

- interactive cleanup for branches whose upstream tracking ref is `[gone]`
- `--batch` for scriptable branch-name output
- `--dry-run` for a readable preview without deleting anything

## Requirements

- `git`
- Rust toolchain with `rustfmt` and `clippy`

This repo includes `rust-toolchain.toml` so a standard Rust setup can install the right components automatically.
If you use `mise`, that is still fine; `mise` can manage the Rust toolchain, but no extra repo-specific `mise` config is required here.

## Running locally

```bash
cargo run
```

That starts the interactive TUI.

Other modes:

```bash
cargo run -- --batch
cargo run -- --dry-run
```

## Running the tests

Core local checks:

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

If you want to verify the same steps manually, run the commands in the previous section.

## CI

GitHub Actions runs:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`

Tests run on both Linux and macOS.
