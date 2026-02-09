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
# Dismiss dashboard, switch to Maintenance
Type "D"
Sleep 0.5s
Tab
Sleep 0.5s
Tab
Sleep 0.8s
# Navigate to Log column and drill in
Type "lllllll"
Sleep 0.5s
Enter
Sleep 1.5s
Show

Sleep 1.5s

Screenshot docs/static/images/service-log.png
