# Demo Recording

This repo uses [VHS](https://github.com/charmbracelet/vhs) to produce scripted terminal recordings (GIF + `.cast`) for the README. Scripts live in `docs/demo/`.

## Prerequisites

```bash
brew install vhs
```

VHS also requires a TTY emulator backend. `brew install vhs` pulls in `ttyd` automatically. VHS outputs GIF, MP4, or WebM — it does not support `.cast` (asciinema) format directly.

## Files

| File | Purpose |
|------|---------|
| `docs/demo/setup.sh` | Creates a throwaway GitHub repo with branches in every git-broom category |
| `docs/demo/preview.tape` | Records the `git-broom` preview demo (~8s) |
| `docs/demo/clean.tape` | Records the `git-broom clean` interactive demo (~25s) |

## Recording workflow

1. **Create a demo repo on GitHub** and run the setup script:

   ```bash
   gh repo create <you>/broom-demo --public
   ./docs/demo/setup.sh <you>/broom-demo
   cd /tmp/broom-demo
   ```

2. **Record** each tape:

   ```bash
   vhs ~/src/git-broom/docs/demo/preview.tape
   vhs ~/src/git-broom/docs/demo/clean.tape
   ```

   Outputs land where the tape's `Output` lines specify (relative to cwd).

3. **Review the GIFs**, then adjust `Sleep` durations in the `.tape` files as needed. The interactive cleanup tape is timing-sensitive — if git-broom takes longer to render than expected, increase the `Sleep` after `Enter` so keypresses don't land before the TUI is ready.

4. **Clean up** the demo repo when done:

   ```bash
   gh repo delete <you>/broom-demo --yes
   rm -rf /tmp/broom-demo
   ```

## Tuning tips

- **Typing speed:** `Set TypingSpeed 40ms` in the tape. Lower = faster typing. 30-50ms looks natural.
- **Theme:** `Set Theme "Catppuccin Mocha"`. Run `vhs themes` to see alternatives.
- **Dimensions:** `Set Width 1200` / `Set Height 700`. Match your README's display width — oversized GIFs get scaled down and lose legibility.
- **TUI timing:** The `clean.tape` sends keystrokes (`d`, `j`, `s`, `a`, `y`) to the running TUI. If git-broom's GitHub fetch takes longer than the `Sleep` before the first keystroke, the recording will break. Increase the post-Enter sleep or use `--refresh` to pre-warm the cache before recording.
