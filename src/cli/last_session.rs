use crate::error::Result;
use crate::history::HistoryStore;
use crate::tmux::Tmux;

use super::utils::{record_current_window, sort_windows_by_history, switch_to_window};

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
    pub fn run(&self, client: &dyn Tmux, history: &mut dyn HistoryStore) -> Result<()> {
        let windows = client.list_windows()?;

        if windows.is_empty() {
            client.display_message("No windows found")?;
            return Ok(());
        }

        record_current_window(client, history)?;

        let filtered_windows = if client.is_inside_tmux() {
            let current_session = client.current_session()?;
            windows
                .into_iter()
                .filter(|w| w.session_name != current_session)
                .collect()
        } else {
            windows
        };

        let indexed_windows = sort_windows_by_history(filtered_windows, &*history);

        if let Some((window, _)) = indexed_windows.first() {
            switch_to_window(client, window, history)?;
        } else {
            client.display_message("No previous window found")?;
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
            pane_id: format!("%{session}{index}"),
        }
    }

    #[test]
    fn switches_to_most_recent_window_in_another_session() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        mock.windows = vec![win("s", 1), win("a", 1), win("b", 1)];

        let mut history = InMemoryHistory::seeded(&[("a", 1, 200), ("b", 1, 100)]);
        LastSessionCommand.run(&mock, &mut history).unwrap();

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

        let mut history = InMemoryHistory::new();
        LastSessionCommand.run(&mock, &mut history).unwrap();
        assert!(mock.called("display_message(No previous window found)"));
        assert!(!mock.called("switch_to_window"));
    }

    #[test]
    fn reports_when_no_windows_found() {
        let mut mock = MockTmux::default();
        mock.windows = vec![];
        let mut history = InMemoryHistory::new();
        LastSessionCommand.run(&mock, &mut history).unwrap();
        assert!(mock.called("display_message(No windows found)"));
    }

    /// Jumping s→a with `last-session`, then invoking it again, returns to s:
    /// the most-recently-left session is always the next hop. The store's
    /// monotonic clock makes this deterministic across the two invocations.
    #[test]
    fn toggles_between_sessions_across_invocations() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        mock.windows = vec![win("s", 1), win("a", 1), win("b", 1)];
        let mut history = InMemoryHistory::seeded(&[("a", 1, 200), ("b", 1, 100)]);

        // From s, the most recent other session is a.
        LastSessionCommand.run(&mock, &mut history).unwrap();
        assert!(mock.called("switch_to_window(a,1)"));

        // Now on a; the most recent other session is s (just left).
        mock.current_session = "a".to_string();
        mock.current_window = ("a".to_string(), 1);
        LastSessionCommand.run(&mock, &mut history).unwrap();
        assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,1)");
    }

    /// A newly created window in another session (no history) is still a valid
    /// jump target when it is the only window outside the current session.
    #[test]
    fn new_window_in_other_session_selected_when_only_option() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        mock.windows = vec![win("s", 1), win("newsess", 7)];

        let mut history = InMemoryHistory::new();
        LastSessionCommand.run(&mock, &mut history).unwrap();
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

        let mut history = InMemoryHistory::seeded(&[("a", 1, 500)]);
        LastSessionCommand.run(&mock, &mut history).unwrap();
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

        let mut history = InMemoryHistory::seeded(&[("s", 1, 300), ("a", 1, 100)]);
        LastSessionCommand.run(&mock, &mut history).unwrap();
        assert!(mock.called("attach_to_window(s,1)"));
        assert!(!mock.called("switch_to_window"));
    }
}
