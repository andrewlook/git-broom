# git-broom

`git-broom` is a small Rust CLI for cleaning up stale local git branches after squash-merge workflows leave tracking branches behind.

Current behavior focuses on two cleanup tranches:

- `gone`: branches whose upstream tracking ref is `[gone]`
- `unpushed`: local branches with no upstream configured
- interactive mode walks the selected tranches one by one
- `--batch` prints deletable branch names for the selected tranches
- `--dry-run` prints grouped previews without deleting anything

## Requirements

- `git`
- Rust toolchain with `rustfmt` and `clippy`

This repo includes `rust-toolchain.toml` so a standard Rust setup can install the right components automatically.
If you use `mise`, that is still fine; `mise` can manage the Rust toolchain, but no extra repo-specific `mise` config is required here.

## Running locally

```bash
cargo run
```

That starts the interactive workflow for all implemented cleanup tranches in order.

Other modes:

```bash
cargo run -- gone
cargo run -- unpushed
cargo run -- gone unpushed --dry-run
cargo run -- gone --batch
```

## Install from source

Clone the repo, then install the binary with Cargo:

```bash
cargo install --path .
```

That places `git-broom` in Cargo's bin directory, typically `~/.cargo/bin`.

If you prefer to build it without installing globally:

```bash
cargo build --release
./target/release/git-broom
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
