use crate::error::Result;
use crate::history::WindowHistory;
use crate::paths;
use crate::tmux::TmuxClient;

use super::utils::{sort_windows_by_history, switch_to_window};

pub fn handle(client: &TmuxClient, current_session: bool) -> Result<()> {
    let windows = client.list_windows();

    if windows.is_empty() {
        client.display_message("No windows found")?;
        return Ok(());
    }

    let history_file = paths::history_file_path().to_string_lossy().to_string();
    let mut history = WindowHistory::new(history_file);
    history.load()?;
    history.record_current_window(client)?;

    let mut indexed_windows = sort_windows_by_history(windows, &history);

    if current_session {
        let current_session = client.current_session()?;
        indexed_windows.retain(|(window, _)| window.session_name == current_session);
    }

    // Get the previous window (index 1 = second in sorted list, after current window)
    if let Some((window, _)) = indexed_windows.get(1) {
        switch_to_window(client, window, &mut history)?;
    } else {
        client.display_message("No previous window found")?;
    }

    Ok(())
}
