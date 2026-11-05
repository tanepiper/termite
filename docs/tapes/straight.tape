Output "OUT_DIR/straight.gif"
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
Wait+Screen@90s /Injected/
Sleep 2.5s
Ctrl+U
Type "?x delete the build folder"
Sleep 500ms
Enter
Wait+Screen@90s /y\/N|Y\/n/
Sleep 3s
Type "n"
Sleep 1.5s
