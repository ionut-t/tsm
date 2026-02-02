# tsm - Tmux Session Manager

An overengineered CLI tool for managing tmux sessions because apparently `tmux choose-tree` wasn't fancy enough.

## Features (aka Why I Built This Instead of Using Native Tmux)

- **Fuzzy session/window switching** - Jump to any session or window with live preview (because scrolling is so 2010)
- **Zoxide integration** - Create sessions from frequently used directories (yes, it needed another dependency)
- **Smart history tracking** - Most recently used sessions and windows appear first (finally, a use for all that data hoarding)
- **Quick session/window toggling** - Toggle between last 2 sessions or last 2 windows with shortcuts (Alt+Tab for tmux, basically)
- **Window management** - Move windows between sessions and swap windows within sessions (because clicking is overrated)
- **Workspaces** - Define session layouts in TOML files (because typing commands is for people with free time)

## Requirements

- [tmux](https://github.com/tmux/tmux) - obviously
- [fzf](https://github.com/junegunn/fzf) - for the fuzzy finding magic ✨
- [zoxide](https://github.com/ajeetdsouza/zoxide) - because `cd` is too mainstream
- [bat](https://github.com/sharkdp/bat) - for pretty previews (cat is for animals)

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

# Swap windows in current session
tsm swap-window -t 3            # Swap current window with window 3
tsm swap-window -s 2 -t 5       # Swap window 2 with window 5

# Kill session
tsm kill                        # Fuzzy finder
tsm kill -s myproject           # Direct kill
tsm kll -a                      # Kill all

# Rename session
tsm rename -s mysession -n newname # Rename a session
tsm rename -n newname              # Rename current session

# Workspaces (session templates)
tsm workspace                       # Pick and launch workspace
tsm workspace myproject             # Launch specific workspace
tsm workspace -n custom-name        # Override session name
tsm workspace -p ~/other/path       # Override root directory
tsm workspace new myproject         # Create new workspace (opens editor)
tsm workspace edit myproject        # Edit existing workspace
tsm workspace list                  # List all workspaces
tsm workspace delete myproject      # Delete workspace
tsm workspace path                  # Show workspaces directory
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
- `tsm sww` → `tsm swap-window`
- `tsm ws` → `tsm workspace`

## Workspaces (For the Declarative Obsessed)

Workspaces let you define session layouts in TOML files. Store them in `~/.config/tsm/workspaces/` (or set `TSM_CONFIG_DIR` if you're that person).

```toml
# ~/.config/tsm/workspaces/myproject.toml
name = "myproject"
root = "~/code/myproject"

[env]
NODE_ENV = "development"

[[window]]
name = "code"
focus = true

[[window.row]]

[[window.row.pane]]
command = "nvim ."
focus = true

[[window]]
name = "servers"

[window.env]
PORT = "3000"

[[window.row]]
height = 70

[[window.row.pane]]
command = "cargo watch -x check"
width = 50

[[window.row.pane]]
command = "npm run dev"

[[window.row]]

[[window.row.pane]]
command = "lazygit"
```

This creates a session with two windows: one for coding, one split into rows for running servers. Yes, you could just type these commands manually. But where's the fun in that?

Environment variables cascade down and can be overridden at each level: session `[env]` applies everywhere, window `[window.env]` applies to that window's panes, and `[window.row.pane.env]` applies to a single pane.

**Workspace config location priority:**

1. `TSM_CONFIG_DIR` environment variable
2. `XDG_CONFIG_HOME/tsm/workspaces`
3. `~/.config/tsm/workspaces` (the default for people who don't overthink config paths)

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
bind m command-prompt -p "Swap with window:" "run-shell 'tsm swap-window -t %%'"
bind W display-popup -E -w 80% -h 80% "tsm workspace"

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
- `prefix + m` - Swap current window with another (manual reordering for perfectionists)
- `prefix + W` - Launch workspace (for your carefully crafted TOML files)

**The Hook:**

The `after-select-window` hook tracks all window switches (even native tmux commands like `prefix+w` or `prefix+n`), so `last-window` and `last-session` actually know where you've been. Without it, only switches through tsm get tracked, which is... less useful.

## Shell Completions

```bash
# Bash
tsm completions bash > ~/.local/share/bash-completion/completions/tsm

# Zsh (with zinit)
tsm completions zsh > "${ZINIT[COMPLETIONS_DIR]}/_tsm"

# Zsh (manual)
tsm completions zsh > ~/.zfunc/_tsm  # ensure ~/.zfunc is in the fpath

# Fish
tsm completions fish > ~/.config/fish/completions/tsm.fish
```

Restart shell or run `exec $SHELL` to activate.

## License

[MIT](LICENSE)
