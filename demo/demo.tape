# tsm README demo. Run via ./demo/record.sh (it builds the isolated environment).

Output assets/demo.gif

Set Shell "bash"
Set Theme "Catppuccin Mocha"
# Symbols Nerd Font is a glyph fallback only (eza preview icons); text renders in JetBrains Mono.
Set FontFamily "JetBrains Mono, Symbols Nerd Font Mono"
Set FontSize 16
Set Width 1500
Set Height 720
Set Padding 20
Set TypingSpeed 60ms

# --- Setup (hidden): isolated tmux server + a pre-launched api-gateway session
Hide
Type "demo/.tmp/start.sh"
Enter
Sleep 1.5s
Type "tsm ws api-gateway; tmux switch-client -t scratch; clear"
Enter
Sleep 2.5s
Show

# 1. Workspaces defined as TOML
Type "tsm workspace list"
Sleep 300ms
Enter
Sleep 2s

# 2-3. Launch a workspace from the picker -> multi-pane session
Ctrl+B
Type "W"
Sleep 1.5s
Type "web"
Sleep 1s
Enter
Sleep 3s

# 4. New session from a zoxide directory, with tree preview
Ctrl+B
Type "N"
Sleep 1.2s
Type "infra"
Sleep 1.5s
Enter
Sleep 1.5s

# 5. Switch to any window across sessions, with live pane preview
Ctrl+B
Type "o"
Sleep 1.2s
Type "gate"
Sleep 1.8s
Enter
Sleep 2s

# 6. Toggle back to the last session
Ctrl+B
Type "L"
Sleep 1.5s

# 7. Command palette
Ctrl+B
Type "?"
Sleep 1s
Type "swap"
Sleep 2.5s
Escape
Sleep 500ms
