use crate::error::Result;
use crate::history::WindowHistory;
use crate::paths;
use crate::tmux::TmuxClient;

use super::utils::{sort_windows_by_history, switch_to_window};

/// Switches to the last active window.
///
/// Uses window access history to determine which window was most recently active.
/// Can optionally limit to windows in the current session only.
#[derive(clap::Parser, Debug)]
pub struct LastWindowCommand {
    /// Whether to limit to the current session
    #[clap(short, long, default_value_t = false)]
    current_session: bool,
}

impl LastWindowCommand {
    /// Executes the last window command.
    ///
    /// Switches to the second most recently accessed window (the previous window).
    pub fn run(&self, client: &TmuxClient) -> Result<()> {
        let mut windows = client.list_windows();

        if self.current_session {
            let current_session = client.current_session()?;
            windows.retain(|w| w.session_name == current_session);
        }

        if windows.is_empty() {
            client.display_message("No windows found")?;
            return Ok(());
        }

        let mut history = WindowHistory::new(paths::history_file_path());
        history.load()?;
        history.record_current_window(client)?;

        let indexed_windows = sort_windows_by_history(windows, &history);

        // Get the previous window (index 1 = second in sorted list, after current window)
        if let Some((window, _)) = indexed_windows.get(1) {
            switch_to_window(client, window, &mut history)?;
        } else {
            client.display_message("No previous window found")?;
        }

        Ok(())
    }
}
