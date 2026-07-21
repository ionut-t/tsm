use crate::error::Result;
use crate::history::WindowHistory;
use crate::history::paths;
use crate::tmux::Tmux;

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
    pub fn run(&self, client: &dyn Tmux) -> Result<()> {
        let windows = client.list_windows()?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockTmux, with_env};
    use crate::tmux::Window;
    use std::io::Write;
    use tempfile::TempDir;

    fn win(session: &str, index: u32) -> Window {
        Window {
            session_name: session.to_string(),
            index,
            name: format!("w{index}"),
            pane_id: format!("%{session}{index}"),
        }
    }

    fn with_history(entries: &[(&str, u32, u128)], f: impl FnOnce()) {
        let tmp = TempDir::new().unwrap();
        let hist = tmp.path().join("history");
        {
            let mut file = std::fs::File::create(&hist).unwrap();
            for (s, i, ts) in entries {
                writeln!(file, "{s}:{i}\t{ts}").unwrap();
            }
        }
        with_env(&[("TSM_HISTORY_FILE", hist.to_str())], f);
    }

    #[test]
    fn switches_to_most_recent_window_in_another_session() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        mock.windows = vec![win("s", 1), win("a", 1), win("b", 1)];

        with_history(&[("a", 1, 200), ("b", 1, 100)], || {
            LastSessionCommand.run(&mock).unwrap();
        });

        // Current session "s" is excluded; "a" is more recent than "b".
        assert!(mock.called("switch_to_window(a,1)"));
    }

    #[test]
    fn reports_when_no_other_session_window_exists() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        // Only current-session windows exist, all filtered out.
        mock.windows = vec![win("s", 1), win("s", 2)];

        with_history(&[], || {
            LastSessionCommand.run(&mock).unwrap();
        });
        assert!(mock.called("display_message(No previous window found)"));
        assert!(!mock.called("switch_to_window"));
    }

    #[test]
    fn reports_when_no_windows_found() {
        let mut mock = MockTmux::default();
        mock.windows = vec![];
        with_history(&[], || {
            LastSessionCommand.run(&mock).unwrap();
        });
        assert!(mock.called("display_message(No windows found)"));
    }

    /// Small wall-clock gap so each navigation stamps a strictly newer time,
    /// mirroring real use (see the note in `last_window`).
    fn tick() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    /// Jumping s→a with `last-session`, then invoking it again, returns to s:
    /// the most-recently-left session is always the next hop.
    #[test]
    fn toggles_between_sessions_across_invocations() {
        with_history(&[("a", 1, 200), ("b", 1, 100)], || {
            let mut mock = MockTmux::default();
            mock.current_session = "s".to_string();
            mock.current_window = ("s".to_string(), 1);
            mock.windows = vec![win("s", 1), win("a", 1), win("b", 1)];

            // From s, the most recent other session is a.
            LastSessionCommand.run(&mock).unwrap();
            assert!(mock.called("switch_to_window(a,1)"));

            // Now on a; the most recent other session is s (just left).
            tick();
            mock.current_session = "a".to_string();
            mock.current_window = ("a".to_string(), 1);
            LastSessionCommand.run(&mock).unwrap();
            assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,1)");
        });
    }

    /// A newly created window in another session (no history) is still a valid
    /// jump target when it is the only window outside the current session.
    #[test]
    fn new_window_in_other_session_selected_when_only_option() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        mock.windows = vec![win("s", 1), win("newsess", 7)];

        with_history(&[], || {
            LastSessionCommand.run(&mock).unwrap();
        });
        assert!(mock.called("switch_to_window(newsess,7)"));
    }

    /// Between a visited other-session window and a brand-new one, the visited
    /// window wins.
    #[test]
    fn prefers_visited_other_session_window_over_a_new_one() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        mock.windows = vec![win("s", 1), win("a", 1), win("newsess", 7)];

        with_history(&[("a", 1, 500)], || {
            LastSessionCommand.run(&mock).unwrap();
        });
        assert!(mock.called("switch_to_window(a,1)"));
        assert!(!mock.called("switch_to_window(newsess,7)"));
    }

    /// Outside tmux the current session is unknown, so no session filtering
    /// happens: the pick is the most recent window overall and the client
    /// attaches rather than switches.
    #[test]
    fn outside_tmux_considers_all_windows_and_attaches() {
        let mut mock = MockTmux::default();
        mock.inside_tmux = false;
        mock.windows = vec![win("s", 1), win("a", 1)];

        with_history(&[("s", 1, 300), ("a", 1, 100)], || {
            LastSessionCommand.run(&mock).unwrap();
        });
        assert!(mock.called("attach_to_window(s,1)"));
        assert!(!mock.called("switch_to_window"));
    }
}
