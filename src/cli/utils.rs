use crate::error::Result;
use crate::history::WindowHistory;
use crate::tmux::{TmuxClient, Window};

pub const PREVIEW_CMD: &str = r#"
# Sanitize input to prevent command injection
PANE_ID=$(echo {} | cut -f1 | sed 's/[^a-zA-Z0-9_%@:-]//g')
# Validate that PANE_ID looks like a tmux pane ID (e.g., %1, %2, etc.)
if [[ "$PANE_ID" =~ ^%[0-9]+$ ]]; then
    tmux capture-pane -e -p -t "$PANE_ID" 2>/dev/null || echo "No preview available"
else
    echo "Invalid pane ID"
fi
"#;

pub const PREVIEW_LS_TREE_CMD: &str = r#"
dir={}; dir="${dir/#\~/$HOME}"; command -v eza >/dev/null && eza --color=always --icons=always --tree --level=1 --group-directories-first "$dir" || ls "$dir"
"#;

/// Sort windows by access time (most recent first) and return indexed list
pub fn sort_windows_by_history(
    windows: Vec<Window>,
    history: &WindowHistory,
) -> Vec<(Window, u128)> {
    let mut indexed_windows: Vec<_> = windows
        .into_iter()
        .map(|w| {
            let last_access = history
                .get_last_access(&w.session_name, w.index)
                .unwrap_or(0);
            (w, last_access)
        })
        .collect();
    indexed_windows.sort_by(|a, b| b.1.cmp(&a.1));
    indexed_windows
}

/// Record window access and switch to it
pub fn switch_to_window(
    client: &TmuxClient,
    window: &Window,
    history: &mut WindowHistory,
) -> Result<()> {
    history.record_access(&window.session_name, window.index)?;
    history.save()?;

    if client.is_inside_tmux() {
        client.switch_to_window(&window.session_name, window.index)?;
    } else {
        client.attach_to_window(&window.session_name, window.index)?;
    }

    Ok(())
}
