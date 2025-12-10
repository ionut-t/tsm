use crate::error::Result;
use crate::history::WindowHistory;
use crate::paths;
use crate::tmux::TmuxClient;

use super::utils::{sort_windows_by_history, switch_to_window};

pub fn handle(client: &TmuxClient) -> Result<()> {
    let windows = client.list_windows();

    if windows.is_empty() {
        client.display_message("No windows found")?;
        return Ok(());
    }

    let history_file = paths::history_file_path().to_string_lossy().to_string();
    let mut history = WindowHistory::new(history_file);
    history.load()?;
    history.record_current_window(client)?;

    let filtered_windows = if client.is_inside_tmux() {
        let current_session = client.current_session()?;
        windows
            .into_iter()
            .filter(|w| w.session_name != current_session)
            .collect()
    } else {
        windows
    };

    let indexed_windows = sort_windows_by_history(filtered_windows, &history);

    if let Some((window, _)) = indexed_windows.first() {
        switch_to_window(client, window, &mut history)?;
    } else {
        client.display_message("No previous window found")?;
    }

    Ok(())
}
