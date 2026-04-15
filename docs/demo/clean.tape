# Demo 2: git-broom clean
#
# Shows the interactive cleanup flow — triage, review, confirm.
# Run from inside the demo repo created by setup.sh.
#
# Usage:
#   cd /tmp/<demo-repo>
#   vhs /path/to/clean.tape

Output docs/demo/clean.gif

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 40ms
Set Theme "Catppuccin Mocha"

Sleep 500ms

# Start interactive cleanup for gone + merged groups
Type "git-broom clean -g gone,merged"
Enter

# Wait for TUI to render (fetches GitHub data)
Sleep 3s

# -- GONE group triage --
# Navigate down through branches, mark some for deletion
Sleep 500ms
Type "d"
Sleep 400ms
Type "j"
Sleep 400ms
Type "d"
Sleep 400ms
Type "j"
Sleep 400ms

# Save one branch to show the save feature
Type "s"
Sleep 800ms

# Proceed to review
Enter
Sleep 2s

# Confirm deletion
Type "y"
Sleep 2s

# -- MERGED group triage --
# Use "mark all" to select everything at once
Sleep 500ms
Type "a"
Sleep 800ms

# Proceed to review
Enter
Sleep 2s

# Confirm deletion
Type "y"
Sleep 2s

# Show the result
Type "git branch"
Enter
Sleep 3s
