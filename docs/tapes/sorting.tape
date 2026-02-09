Require micasa

Set Shell bash
Set FontFamily "JetBrains Mono"
Set FontSize 14
Set Width 1400
Set Height 900
Set Padding 20
Set Theme "Dracula"
Set CursorBlink false

Env NO_COLOR ""
Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env COLORFGBG "15;0"

Hide
Type "micasa --demo"
Enter
Sleep 4s
# Dismiss dashboard, sort by Title asc then Status asc
Type "D"
Sleep 0.5s
Type "ll"
Sleep 0.3s
Type "s"
Sleep 0.5s
Type "l"
Sleep 0.3s
Type "s"
Sleep 0.8s
Show

Sleep 1.5s

Screenshot docs/static/images/sorting.png
