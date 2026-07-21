use crate::{error::Result, history::HistoryStore, tmux::Tmux};

/// Records the current window access in the history file.
///
/// This command is typically used in tmux hooks to track window access times.
#[derive(clap::Parser, Debug)]
pub struct RecordCommand;

impl RecordCommand {
    /// Executes the record command.
    ///
    /// Records the current window access time in the history file.
    pub fn run(&self, client: &dyn Tmux, history: &mut dyn HistoryStore) -> Result<()> {
        if !client.is_inside_tmux() {
            return Ok(());
        }

        let (session, window) = client.get_current_window()?;
        history.record(&session, window)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{InMemoryHistory, MockTmux};

    #[test]
    fn does_nothing_when_outside_tmux() {
        let mut mock = MockTmux::default();
        mock.inside_tmux = false;
        mock.current_window = ("dev".to_string(), 4);

        let mut history = InMemoryHistory::new();
        RecordCommand.run(&mock, &mut history).unwrap();
        // Nothing is recorded when not inside tmux.
        assert!(history.last_access("dev", 4).is_none());
    }

    #[test]
    fn records_current_window_to_history() {
        let mut mock = MockTmux::default();
        mock.current_window = ("dev".to_string(), 4);

        let mut history = InMemoryHistory::new();
        RecordCommand.run(&mock, &mut history).unwrap();
        assert!(history.last_access("dev", 4).is_some());
    }
}
