use crate::error::Result;
use crate::history::WindowHistory;
use crate::tmux::{TmuxClient, Window};

pub const PREVIEW_CMD: &str = r#"
PANE_ID=$(echo {} | cut -f1)
tmux capture-pane -e -p -t "$PANE_ID" 2>/dev/null || echo "No preview available"
"#;

/// Sort windows by access time (most recent first) and return indexed list
pub fn sort_windows_by_history(
    windows: Vec<Window>,
    history: &WindowHistory,
) -> Vec<(Window, u64)> {
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
    history.record_access(&window.session_name, window.index);
    history.save()?;

    if client.is_inside_tmux() {
        client.switch_to_window(&window.session_name, window.index)?;
    } else {
        client.attach_to_window(&window.session_name, window.index)?;
    }

    Ok(())
}
