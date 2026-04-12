# git-broom

`git-broom` is a small Rust CLI for cleaning up stale local git branches after squash-merge workflows leave tracking branches behind.

Current behavior focuses on three cleanup modes:

- `gone`: branches whose upstream tracking ref is `[gone]`
- `unpushed`: local branches with no upstream configured
- `closed`: remote-tracked branches whose PR is closed or missing on GitHub
- interactive mode walks the selected groups one by one
- `--batch` prints the same readable deletion preview without entering the TUI
- `--dry-run` prints grouped previews without deleting anything
- `s` in interactive mode saves or unsaves a branch for this repo and cleanup mode

## Requirements

- `git`
- Rust toolchain with `rustfmt` and `clippy`
- `gh` with an authenticated session if you want to use `closed` mode

This repo includes `rust-toolchain.toml` so a standard Rust setup can install the right components automatically.
If you use `mise`, that is still fine; `mise` can manage the Rust toolchain, but no extra repo-specific `mise` config is required here.

## Running locally

```bash
cargo run
```

That starts the interactive workflow for all implemented cleanup groups in order.

Other modes:

```bash
cargo run -- gone
cargo run -- unpushed
cargo run -- closed
cargo run -- gone unpushed --dry-run
cargo run -- closed --remote upstream
cargo run -- gone --batch
```

`closed` mode can expand into more than one review group, for example a `closed` group for closed/no-PR branches and a `merged` group for merged PRs whose remote branch still exists.

Saved branches are cached locally under the repo's git metadata directory, not in tracked files. In a normal clone that path is `.git/git-broom/keep-labels.json`; in worktree setups it resolves through the shared git common dir.

Within each review group, branches are shown in this order:

- protected branches first
- saved branches next
- regular cleanup candidates last

Saved branches stay visible in both the TUI and preview output, but `delete all` skips them until you unsave them.

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
