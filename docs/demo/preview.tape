# Demo 1: git-broom preview
#
# Shows the grouped branch overview — the "what have I got?" moment.
# Run from inside the demo repo created by setup.sh.
#
# Usage:
#   cd /tmp/<demo-repo>
#   vhs /path/to/preview.tape

Output docs/demo/preview.gif

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 40ms
Set Theme "Catppuccin Mocha"

# Brief pause so the viewer can orient
Sleep 500ms

# Show the branch mess
Type "git branch"
Enter
Sleep 2s

# Run git-broom to see the grouped view
Type "git-broom"
Enter
Sleep 5s
