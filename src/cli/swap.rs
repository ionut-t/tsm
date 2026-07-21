use crate::error::Result;
use crate::error::TsmError;
use crate::history::WindowHistory;
use crate::history::paths;
use crate::tmux::Tmux;

/// Swaps the positions of two windows within the current session.
///
/// The source window defaults to the current window if not specified.
/// Both windows must be in the same session.
#[derive(clap::Parser, Debug)]
pub struct SwapWindowCommand {
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

impl SwapWindowCommand {
    /// Executes the swap window command.
    ///
    /// Swaps the source and target windows and switches to the new position of the current window.
    pub fn run(&self, client: &dyn Tmux) -> Result<()> {
        if !client.is_inside_tmux() {
            return Err(TsmError::NotInTmux);
        }

        let source_index = match self.source {
            Some(index) => index,
            None => {
                let (_, window_index) = client.get_current_window()?;
                window_index
            }
        };

        if source_index == self.target {
            client.display_message("Source and target window indices are the same")?;
            return Ok(());
        }

        let session = client.current_session()?;
        let all_windows = client.list_windows()?;
        let session_windows: Vec<_> = all_windows
            .into_iter()
            .filter(|w| w.session_name == session)
            .map(|w| w.index)
            .collect();

        if session_windows.len() < 2 {
            client.display_message("Not enough windows in the current session to perform swap.")?;
            return Ok(());
        }

        if !session_windows.contains(&source_index) {
            client.display_message(&format!(
                "Window {} not found in current session",
                source_index
            ))?;
            return Ok(());
        }

        if !session_windows.contains(&self.target) {
            client.display_message(&format!(
                "Window {} not found in current session",
                self.target
            ))?;
            return Ok(());
        }

        let (_, current_window_index) = client.get_current_window()?;

        client.swap_windows(source_index, self.target)?;

        if source_index == current_window_index {
            client.switch_to_window(&session, self.target)?;

            let mut history = WindowHistory::new(paths::history_file_path());
            history.load()?;
            history.record_access(&session, self.target)?;
            history.save()?;
        }

        if !self.quiet {
            client.display_message(&format!(
                "Swapped windows {} and {}",
                source_index, self.target,
            ))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockTmux, with_env};
    use crate::tmux::Window;
    use tempfile::TempDir;

    fn win(session: &str, index: u32) -> Window {
        Window {
            session_name: session.to_string(),
            index,
            name: format!("w{index}"),
            pane_id: format!("%{index}"),
        }
    }

    fn cmd(source: Option<u32>, target: u32, quiet: bool) -> SwapWindowCommand {
        SwapWindowCommand {
            source,
            target,
            quiet,
        }
    }

    /// Runs `f` with `TSM_HISTORY_FILE` pointed at a throwaway file so history
    /// writes never touch the real state directory.
    fn with_temp_history(f: impl FnOnce()) {
        let tmp = TempDir::new().unwrap();
        let hist = tmp.path().join("history");
        with_env(&[("TSM_HISTORY_FILE", hist.to_str())], f);
    }

    #[test]
    fn errors_when_not_inside_tmux() {
        let mut mock = MockTmux::default();
        mock.inside_tmux = false;
        let err = cmd(Some(1), 2, false).run(&mock).unwrap_err();
        assert!(matches!(err, TsmError::NotInTmux));
    }

    #[test]
    fn no_op_when_source_equals_target() {
        let mock = MockTmux::default();
        with_temp_history(|| {
            cmd(Some(2), 2, false).run(&mock).unwrap();
        });
        assert!(!mock.called("swap_windows"));
        assert!(mock.called("display_message(Source and target"));
    }

    #[test]
    fn swaps_and_follows_current_window() {
        let mut mock = MockTmux::default();
        mock.current_session = "dev".to_string();
        mock.current_window = ("dev".to_string(), 1);
        mock.windows = vec![win("dev", 1), win("dev", 2)];

        with_temp_history(|| {
            // Source 1 is the current window, so after swapping to 2 the command
            // follows it there.
            cmd(Some(1), 2, false).run(&mock).unwrap();
        });

        assert!(mock.called("swap_windows(1,2)"));
        assert!(mock.called("switch_to_window(dev,2)"));
        assert!(mock.called("display_message(Swapped windows 1 and 2)"));
    }

    #[test]
    fn reports_missing_window() {
        let mut mock = MockTmux::default();
        mock.current_session = "dev".to_string();
        mock.current_window = ("dev".to_string(), 1);
        mock.windows = vec![win("dev", 1), win("dev", 2)];

        with_temp_history(|| {
            cmd(Some(9), 2, false).run(&mock).unwrap();
        });
        assert!(!mock.called("swap_windows"));
        assert!(mock.called("display_message(Window 9 not found"));
    }

    #[test]
    fn quiet_suppresses_success_message() {
        let mut mock = MockTmux::default();
        mock.current_session = "dev".to_string();
        mock.current_window = ("dev".to_string(), 3);
        mock.windows = vec![win("dev", 1), win("dev", 2)];

        with_temp_history(|| {
            // Current window (3) is neither source nor target, so no follow/switch.
            cmd(Some(1), 2, true).run(&mock).unwrap();
        });
        assert!(mock.called("swap_windows(1,2)"));
        assert!(!mock.called("display_message"));
    }

    #[test]
    fn source_defaults_to_current_window_when_omitted() {
        let mut mock = MockTmux::default();
        mock.current_session = "dev".to_string();
        mock.current_window = ("dev".to_string(), 1);
        mock.windows = vec![win("dev", 1), win("dev", 2)];

        with_temp_history(|| {
            // No source given → the current window (1) is swapped with 2.
            cmd(None, 2, false).run(&mock).unwrap();
        });
        assert!(mock.called("swap_windows(1,2)"));
        assert!(mock.called("switch_to_window(dev,2)"));
    }

    #[test]
    fn reports_when_session_has_fewer_than_two_windows() {
        let mut mock = MockTmux::default();
        mock.current_session = "dev".to_string();
        mock.current_window = ("dev".to_string(), 1);
        mock.windows = vec![win("dev", 1)];

        with_temp_history(|| {
            cmd(Some(1), 2, false).run(&mock).unwrap();
        });
        assert!(!mock.called("swap_windows"));
        assert!(mock.called("display_message(Not enough windows"));
    }

    #[test]
    fn reports_missing_target_window() {
        let mut mock = MockTmux::default();
        mock.current_session = "dev".to_string();
        mock.current_window = ("dev".to_string(), 1);
        mock.windows = vec![win("dev", 1), win("dev", 2)];

        with_temp_history(|| {
            cmd(Some(1), 9, false).run(&mock).unwrap();
        });
        assert!(!mock.called("swap_windows"));
        assert!(mock.called("display_message(Window 9 not found"));
    }
}
