use crate::{error::Result, history::WindowHistory, paths, tmux::TmuxClient};

/// Records the current window access in the history file.
///
/// This command is typically used in tmux hooks to track window access times.
#[derive(clap::Parser, Debug)]
pub struct RecordCommand;

impl RecordCommand {
    /// Executes the record command.
    ///
    /// Records the current window access time in the history file.
    pub fn run(&self, client: &TmuxClient) -> Result<()> {
        if !client.is_inside_tmux() {
            return Ok(());
        }

        let (session, window) = client.get_current_window()?;

        let mut history = WindowHistory::new(paths::history_file_path());
        history.load()?;
        history.record_access(&session, window);
        history.save()?;

        Ok(())
    }
}
