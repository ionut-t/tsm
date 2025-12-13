# tsm - Tmux Session Manager

An overengineered CLI tool for managing tmux sessions because apparently `tmux choose-tree` wasn't fancy enough.

## Features (aka Why I Built This Instead of Using Native Tmux)

- **Fuzzy session/window switching** - Jump to any session or window with live preview (because scrolling is so 2010)
- **Zoxide integration** - Create sessions from frequently used directories (yes, it needed another dependency)
- **Smart history tracking** - Most recently used sessions and windows appear first (finally, a use for all that data hoarding)
- **Quick session/window toggling** - Toggle between last 2 sessions or last 2 windows with shortcuts (Alt+Tab for tmux, basically)
- **Window management** - Move windows between sessions with automatic tracking and following (because dragging is for GUIs)

## Requirements

- [tmux](https://github.com/tmux/tmux) - obviously
- [fzf](https://github.com/junegunn/fzf) - for the fuzzy finding magic ✨
- [zoxide](https://github.com/ajeetdsouza/zoxide) - because `cd` is too mainstream

## Installation (The Rust Way™)

```bash
cargo install --git https://github.com/ionut-t/tsm
```

Or if you're feeling adventurous, clone and build locally:

```bash
git clone https://github.com/ionut-t/tsm
cd tsm
cargo install --path .
```

Now watch it compile for 30 seconds while Rust ensures memory safety.

## Usage

```bash
# Create new session (opens zoxide directory picker)
tsm new
tsm new -n myproject           # With custom name
tsm new -p ~/code/project      # From specific path

# Switch sessions
tsm switch                      # Fuzzy finder
tsm switch -n myproject         # Direct switch

# Switch windows (across all sessions)
tsm switch-window --preview     # Fuzzy finder with preview
tsm last-window                 # Toggle to last active window
tsm last-session                # Toggle to last active window in last active session

# Move windows between sessions
tsm move-window                 # Interactive: pick window + target session
tsm move-window -t backend      # Move current window to "backend" session
tsm move-window -f frontend:3 -t backend  # Move specific window

# Kill session
tsm kill                        # Fuzzy finder
tsm kill -s myproject           # Direct kill
tsm kll -a                      # Kill all

# Rename session
tsm rename -s mysession -n newname # Rename a session
tsm rename -n newname              # Rename current session
```

## Aliases

Most commands have short aliases:

- `tsm n` → `tsm new`
- `tsm s` → `tsm switch`
- `tsm sw` → `tsm switch-window`
- `tsm k` → `tsm kill`
- `tsm r` → `tsm rename`
- `tsm lw` → `tsm last-window`
- `tsm ls` → `tsm last-session`
- `tsm mv` → `tsm move-window`

## Tmux Integration (The Cool Part)

Add these keybindings to `~/.tmux.conf` and feel like a hacker:

```tmux
# Session manager (tsm)
bind o display-popup -E -w 80% -h 80% "tsm switch-window --preview"
bind O display-popup -E -w 40% -h 40% "tsm switch"
bind k display-popup -E -w 40% -h 40% "tsm kill"
bind N display-popup -E -w 80% -h 80% "tsm new --preview"
bind L run-shell "tsm last-session"
bind l run-shell "tsm last-window"
bind M display-popup -E -w 80% -h 80% "tsm move-window"

# Track window switches (makes last-window/last-session actually useful)
set-hook -g after-select-window 'run-shell "tsm record"'
```

**Keybindings:**

- `prefix + o` - Switch window with preview (finally, a good use for popups)
- `prefix + O` - Switch session (capital O for important stuff)
- `prefix + k` - Kill session (with prejudice)
- `prefix + N` - Create new session (because you need _another_ project opened)
- `prefix + L` - Toggle to last session (Alt+Tab, but make it tmux)
- `prefix + l` - Toggle to last window (now you can be indecisive faster)
- `prefix + M` - Move window to another session (for when you put things in the wrong place)

**The Hook:**

The `after-select-window` hook tracks all window switches (even native tmux commands like `prefix+w` or `prefix+n`), so `last-window` and `last-session` actually know where you've been. Without it, only switches through tsm get tracked, which is... less useful.

## License

[MIT](LICENSE)
