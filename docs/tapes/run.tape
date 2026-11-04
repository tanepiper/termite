Output "OUT_DIR/run.gif"
Set Shell zsh
Set FontSize 20
Set Width 1200
Set Height 520
Set Padding 28
Set Theme "Catppuccin Mocha"
Set TypingSpeed 45ms
Hide
Type "source DEMO_DIR/zshrc && clear" Enter
Sleep 1s
Show
Sleep 600ms
Type "?x show disk usage of this folder"
Sleep 500ms
Enter
Wait+Screen@60s /only reads/
Sleep 2s
Type "?x delete the build folder"
Sleep 500ms
Enter
Wait+Screen@60s /Not run|asking/
Sleep 3.5s
