use crate::error::Result;
use crate::history::WindowHistory;
use crate::paths;
use crate::tmux::TmuxClient;

use super::utils::{sort_windows_by_history, switch_to_window};

/// Switches to the last active window in a different session.
///
/// Uses window access history to determine which window was most recently active
/// in a session other than the current one.
#[derive(clap::Parser, Debug)]
pub struct LastSessionCommand;

impl LastSessionCommand {
    /// Executes the last session command.
    ///
    /// Switches to the most recently accessed window in a different session.
    pub fn run(&self, client: &TmuxClient) -> Result<()> {
        let windows = client.list_windows();

        if windows.is_empty() {
            client.display_message("No windows found")?;
            return Ok(());
        }

        let mut history = WindowHistory::new(paths::history_file_path());
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
}
