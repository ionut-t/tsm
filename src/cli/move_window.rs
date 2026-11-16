use crate::error::Result;
use crate::error::TsmError;
use crate::history::HistoryStore;
use crate::tmux::Tmux;

/// Moves a window to a given index within the current session, shifting the
/// windows in between by one position.
///
/// The source window defaults to the current window if not specified.
/// Both indices must already exist in the current session.
#[derive(clap::Parser, Debug)]
pub struct MoveWindowCommand {
    /// Source window index (defaults to the current window)
    #[clap(short, long)]
    source: Option<u32>,

    /// Target window index
    #[clap(short, long)]
    target: u32,

    /// No success message
    #[clap(short = 'q', long, default_value_t = false)]
    quiet: bool,
}

impl MoveWindowCommand {
    /// Executes the move window command.
    ///
    /// Moves the source window to the target index, then keeps focus on the
    /// window that was current before the move (its index may have shifted).
    pub fn run(&self, client: &dyn Tmux, history: &mut dyn HistoryStore) -> Result<()> {
        if !client.is_inside_tmux() {
            return Err(TsmError::NotInTmux);
        }

        let (session, current_index) = client.get_current_window()?;
        let source_index = self.source.unwrap_or(current_index);

        if source_index == self.target {
            client.display_message("Source and target window indices are the same")?;
            return Ok(());
        }

        let session_windows: Vec<u32> = client
            .list_windows()?
            .into_iter()
            .filter(|w| w.session_name == session)
            .map(|w| w.index)
            .collect();

        if let Some(missing) = [source_index, self.target]
            .into_iter()
            .find(|index| !session_windows.contains(index))
        {
            client.display_message(&format!("Window {missing} not found in current session"))?;
            return Ok(());
        }

        // Track the current window by pane id: the move may shift its index.
        let pane_id = client.get_pane_id(&session, current_index)?;
        client.move_window(&session, source_index, self.target)?;

        let (_, new_index) = client.find_window_by_pane_id(&pane_id)?;
        client.switch_to_window(&session, new_index)?;
        history.record(&session, new_index)?;

        if !self.quiet {
            client.display_message(&format!("Moved window {source_index} to {}", self.target))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{InMemoryHistory, MockTmux};
    use crate::tmux::Window;

    fn win(session: &str, index: u32) -> Window {
        Window {
            session_name: session.to_string(),
            index,
            name: format!("w{index}"),
            pane_id: format!("%{index}"),
        }
    }

    fn cmd(source: Option<u32>, target: u32, quiet: bool) -> MoveWindowCommand {
        MoveWindowCommand {
            source,
            target,
            quiet,
        }
    }

    /// Session "dev" with windows 1..=5 (pane ids %1..%5), current window `current`.
    fn dev_session(current: u32) -> MockTmux {
        let mut mock = MockTmux::default();
        mock.current_session = "dev".to_string();
        mock.current_window = ("dev".to_string(), current);
        mock.windows = (1..=5).map(|i| win("dev", i)).collect();
        mock
    }

    #[test]
    fn errors_when_not_inside_tmux() {
        let mut mock = MockTmux::default();
        mock.inside_tmux = false;
        let mut history = InMemoryHistory::new();
        let err = cmd(Some(1), 2, false).run(&mock, &mut history).unwrap_err();
        assert!(matches!(err, TsmError::NotInTmux));
    }

    #[test]
    fn no_op_when_source_equals_target() {
        let mock = dev_session(2);
        let mut history = InMemoryHistory::new();
        // Source defaults to the current window (2), same as the target.
        cmd(None, 2, false).run(&mock, &mut history).unwrap();
        assert!(!mock.called("move_window"));
        assert!(mock.called("display_message(Source and target"));
    }

    #[test]
    fn reports_missing_source_window() {
        let mock = dev_session(1);
        let mut history = InMemoryHistory::new();
        cmd(Some(9), 2, false).run(&mock, &mut history).unwrap();
        assert!(!mock.called("move_window"));
        assert!(mock.called("display_message(Window 9 not found"));
    }

    #[test]
    fn reports_missing_target_window() {
        let mock = dev_session(1);
        let mut history = InMemoryHistory::new();
        cmd(Some(1), 9, false).run(&mock, &mut history).unwrap();
        assert!(!mock.called("move_window"));
        assert!(mock.called("display_message(Window 9 not found"));
    }

    #[test]
    fn ignores_windows_from_other_sessions() {
        let mut mock = dev_session(1);
        mock.windows.push(win("prod", 7));
        let mut history = InMemoryHistory::new();
        // Index 7 exists, but only in "prod".
        cmd(Some(1), 7, false).run(&mock, &mut history).unwrap();
        assert!(!mock.called("move_window"));
        assert!(mock.called("display_message(Window 7 not found"));
    }

    #[test]
    fn moves_current_window_and_follows_it() {
        let mut mock = dev_session(2);
        // After moving 2 -> 4, the current window's pane lives at index 4.
        mock.pane_id = "%4".to_string();
        let mut history = InMemoryHistory::new();

        cmd(None, 4, false).run(&mock, &mut history).unwrap();

        assert!(mock.called("move_window(dev:2->4)"));
        assert!(mock.called("switch_to_window(dev,4)"));
        assert!(history.last_access("dev", 4).is_some());
        assert!(mock.called("display_message(Moved window 2 to 4)"));
    }

    #[test]
    fn follows_current_window_when_it_is_shifted() {
        // Current window is 3; moving 5 -> 1 shifts it to 4. Focus must follow
        // the window (by pane id), not stay on index 3 or jump to the target.
        let mut mock = dev_session(3);
        mock.pane_id = "%4".to_string();
        let mut history = InMemoryHistory::new();

        cmd(Some(5), 1, false).run(&mock, &mut history).unwrap();

        assert!(mock.called("move_window(dev:5->1)"));
        assert!(mock.called("switch_to_window(dev,4)"));
        assert!(!mock.called("switch_to_window(dev,1)"));
        assert!(history.last_access("dev", 4).is_some());
    }

    #[test]
    fn switches_after_moving() {
        let mut mock = dev_session(1);
        mock.pane_id = "%3".to_string();
        let mut history = InMemoryHistory::new();

        cmd(Some(1), 3, false).run(&mock, &mut history).unwrap();

        // The switch must come after the move, otherwise it targets the
        // pre-move layout.
        let calls = mock.calls();
        let move_pos = calls.iter().position(|c| c == "move_window(dev:1->3)");
        let switch_pos = calls.iter().position(|c| c == "switch_to_window(dev,3)");
        assert!(move_pos.is_some() && switch_pos.is_some());
        assert!(move_pos < switch_pos, "switch must follow the move");
    }

    #[test]
    fn quiet_suppresses_success_message() {
        let mut mock = dev_session(1);
        mock.pane_id = "%2".to_string();
        let mut history = InMemoryHistory::new();

        cmd(Some(1), 2, true).run(&mock, &mut history).unwrap();

        assert!(mock.called("move_window(dev:1->2)"));
        assert!(!mock.called("display_message"));
    }
}
