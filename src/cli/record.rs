use crate::{
    error::Result,
    history::{WindowHistory, paths},
    tmux::Tmux,
};

/// Records the current window access in the history file.
///
/// This command is typically used in tmux hooks to track window access times.
#[derive(clap::Parser, Debug)]
pub struct RecordCommand;

impl RecordCommand {
    /// Executes the record command.
    ///
    /// Records the current window access time in the history file.
    pub fn run(&self, client: &dyn Tmux) -> Result<()> {
        if !client.is_inside_tmux() {
            return Ok(());
        }

        let (session, window) = client.get_current_window()?;

        let mut history = WindowHistory::new(paths::history_file_path());
        history.load()?;
        history.record_access(&session, window)?;
        history.save()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockTmux, with_env};
    use tempfile::TempDir;

    #[test]
    fn does_nothing_when_outside_tmux() {
        let tmp = TempDir::new().unwrap();
        let hist = tmp.path().join("history");
        let mut mock = MockTmux::default();
        mock.inside_tmux = false;

        with_env(&[("TSM_HISTORY_FILE", hist.to_str())], || {
            RecordCommand.run(&mock).unwrap();
        });
        // No history file is written when not inside tmux.
        assert!(!hist.exists());
    }

    #[test]
    fn records_current_window_to_history() {
        let tmp = TempDir::new().unwrap();
        let hist = tmp.path().join("history");
        let mut mock = MockTmux::default();
        mock.current_window = ("dev".to_string(), 4);

        with_env(&[("TSM_HISTORY_FILE", hist.to_str())], || {
            RecordCommand.run(&mock).unwrap();

            let mut reloaded = WindowHistory::new(hist.clone());
            reloaded.load().unwrap();
            assert!(reloaded.get_last_access("dev", 4).is_some());
        });
    }
}
