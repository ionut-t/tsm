use crate::error::Result;
use crate::history::WindowHistory;
use crate::history::paths;
use crate::tmux::Tmux;

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
    pub fn run(&self, client: &dyn Tmux) -> Result<()> {
        let mut windows = client.list_windows()?;

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

    /// Run `f` with a preloaded history file at `TSM_HISTORY_FILE`.
    fn with_history(entries: &[(&str, u32, u128)], f: impl FnOnce()) {
        let tmp = TempDir::new().unwrap();
        let hist = tmp.path().join("history");
        {
            let mut file = std::fs::File::create(&hist).unwrap();
            for (s, i, ts) in entries {
                writeln!(file, "{s}:{i}\t{ts}").unwrap();
            }
        }
        // Keep the TempDir alive for the duration of the closure.
        with_env(&[("TSM_HISTORY_FILE", hist.to_str())], f);
    }

    #[test]
    fn switches_to_second_most_recent_window() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        mock.windows = vec![win("s", 1), win("s", 2), win("s", 3)];

        // record_current_window stamps window 1 as "now" (most recent), so the
        // previous window is the next-most-recent in history: window 2 (ts 200).
        with_history(&[("s", 2, 200), ("s", 3, 100)], || {
            LastWindowCommand {
                current_session: false,
            }
            .run(&mock)
            .unwrap();
        });

        assert!(mock.called("switch_to_window(s,2)"));
    }

    #[test]
    fn reports_when_no_previous_window_exists() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        mock.windows = vec![win("s", 1)];

        with_history(&[], || {
            LastWindowCommand {
                current_session: false,
            }
            .run(&mock)
            .unwrap();
        });

        assert!(mock.called("display_message(No previous window found)"));
        assert!(!mock.called("switch_to_window"));
    }

    #[test]
    fn current_session_flag_filters_to_active_session() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        // A window in another session must be excluded by the filter.
        mock.windows = vec![win("s", 1), win("s", 2), win("other", 9)];

        with_history(&[("s", 2, 200), ("other", 9, 999)], || {
            LastWindowCommand {
                current_session: true,
            }
            .run(&mock)
            .unwrap();
        });

        // Despite "other:9" being most recent in history, it's filtered out, so
        // the previous window resolves to s:2.
        assert!(mock.called("switch_to_window(s,2)"));
        assert!(!mock.called("switch_to_window(other,9)"));
    }

    #[test]
    fn reports_when_no_windows_found() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.windows = vec![];

        with_history(&[], || {
            LastWindowCommand {
                current_session: false,
            }
            .run(&mock)
            .unwrap();
        });
        assert!(mock.called("display_message(No windows found)"));
    }

    fn last_window(current_session: bool) -> LastWindowCommand {
        LastWindowCommand { current_session }
    }

    /// Model of the wall-clock gap between two user navigations. History
    /// timestamps have millisecond resolution, so a short pause guarantees each
    /// navigation stamps a strictly newer time than the last — exactly what
    /// happens in real use, without depending on test execution speed.
    fn tick() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    /// The core "toggle" guarantee: after jumping A→B, `last-window` jumps back
    /// to A, and doing it again returns to B — across sequential invocations
    /// sharing the same on-disk history.
    #[test]
    fn toggles_back_and_forth_between_two_windows() {
        with_history(&[("s", 2, 200), ("s", 3, 100)], || {
            let mut mock = MockTmux::default();
            mock.current_session = "s".to_string();
            mock.windows = vec![win("s", 1), win("s", 2), win("s", 3)];

            // Currently on window 1; previous is the most-recent other window (2).
            mock.current_window = ("s".to_string(), 1);
            last_window(false).run(&mock).unwrap();
            assert!(mock.called("switch_to_window(s,2)"));

            // Simulate the switch: now on 2. last-window must return to 1, which
            // was just stamped as the most recent when we left it.
            tick();
            mock.current_window = ("s".to_string(), 2);
            last_window(false).run(&mock).unwrap();
            assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,1)");

            // And back to 2 again.
            tick();
            mock.current_window = ("s".to_string(), 1);
            last_window(false).run(&mock).unwrap();
            assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,2)");
        });
    }

    /// A freshly created window has no history entry (timestamp 0), so it must
    /// never be preferred over a window the user has actually visited.
    #[test]
    fn newly_created_window_is_not_preferred_over_a_visited_window() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        // Window 2 is brand new (absent from history); window 3 was visited.
        mock.windows = vec![win("s", 1), win("s", 2), win("s", 3)];

        with_history(&[("s", 3, 150)], || {
            last_window(false).run(&mock).unwrap();
        });

        assert!(mock.called("switch_to_window(s,3)"));
        assert!(!mock.called("switch_to_window(s,2)"));
    }

    /// But a new window *is* reachable when it is the only alternative to the
    /// current one.
    #[test]
    fn newly_created_window_is_chosen_when_it_is_the_only_other_window() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        mock.windows = vec![win("s", 1), win("s", 2)];

        with_history(&[], || {
            last_window(false).run(&mock).unwrap();
        });
        assert!(mock.called("switch_to_window(s,2)"));
    }

    /// When several new windows tie at timestamp 0, the previous-window pick is
    /// the first one in tmux's window listing order (stable sort), not the
    /// lowest index.
    #[test]
    fn ties_between_new_windows_resolve_by_listing_order() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        // Listing order deliberately not sorted by index: 3 comes before 2.
        mock.windows = vec![win("s", 1), win("s", 3), win("s", 2), win("s", 4)];

        with_history(&[], || {
            last_window(false).run(&mock).unwrap();
        });
        // 3 is first among the zero-history windows in listing order.
        assert!(mock.called("switch_to_window(s,3)"));
        assert!(!mock.called("switch_to_window(s,2)"));
    }

    /// Outside tmux the current window is unknown, so it is never stamped; the
    /// pick comes purely from stored history and the client *attaches* rather
    /// than switches.
    #[test]
    fn outside_tmux_attaches_to_second_most_recent_without_stamping_current() {
        let mut mock = MockTmux::default();
        mock.inside_tmux = false;
        mock.windows = vec![win("s", 1), win("s", 2), win("s", 3)];

        with_history(&[("s", 1, 300), ("s", 2, 200), ("s", 3, 100)], || {
            last_window(false).run(&mock).unwrap();
        });

        assert!(mock.called("attach_to_window(s,2)"));
        assert!(!mock.called("switch_to_window"));
    }

    /// With `--current-session`, windows in other sessions are excluded even
    /// when they are more recent — including brand-new ones — so navigation
    /// stays within the active session.
    #[test]
    fn current_session_filter_keeps_navigation_within_active_session() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        // other:9 is the most recent overall but belongs to another session;
        // s:2 is a new window in the active session.
        mock.windows = vec![win("s", 1), win("s", 2), win("other", 9)];

        with_history(&[("other", 9, 999)], || {
            last_window(true).run(&mock).unwrap();
        });

        assert!(mock.called("switch_to_window(s,2)"));
        assert!(!mock.called("switch_to_window(other,9)"));
    }

    /// A window created in the background (no history) while the user stays on
    /// an existing window gets *adopted* into the toggle: the first last-window
    /// jumps to it, and from then on the two ping-pong stably.
    #[test]
    fn ping_pong_adopts_a_new_background_window() {
        with_history(&[("s", 1, 500)], || {
            let mut mock = MockTmux::default();
            mock.current_session = "s".to_string();
            mock.windows = vec![win("s", 1), win("s", 2)];

            // On existing s:1; the only other candidate is the new window s:2.
            mock.current_window = ("s".to_string(), 1);
            last_window(false).run(&mock).unwrap();
            assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,2)");

            // Now on the newly adopted window; last-window returns to s:1.
            tick();
            mock.current_window = ("s".to_string(), 2);
            last_window(false).run(&mock).unwrap();
            assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,1)");

            // Both have history now — the toggle is stable back to s:2.
            tick();
            mock.current_window = ("s".to_string(), 1);
            last_window(false).run(&mock).unwrap();
            assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,2)");
        });
    }

    /// The foreground variant: the user is *on* a freshly created window (no
    /// history) and toggles with the existing window they came from.
    #[test]
    fn ping_pong_when_the_current_window_is_brand_new() {
        with_history(&[("s", 1, 300)], || {
            let mut mock = MockTmux::default();
            mock.current_session = "s".to_string();
            mock.windows = vec![win("s", 1), win("s", 2)];

            // On the new window s:2; last-window goes to the existing s:1.
            mock.current_window = ("s".to_string(), 2);
            last_window(false).run(&mock).unwrap();
            assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,1)");

            // And back to the new window.
            tick();
            mock.current_window = ("s".to_string(), 1);
            last_window(false).run(&mock).unwrap();
            assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,2)");
        });
    }

    /// Bootstrapping a toggle between two windows that both start with no
    /// history: the current window stamps itself, so the other is always the
    /// "previous" one, and after the first hop both have history.
    #[test]
    fn ping_pong_between_two_initially_new_windows() {
        with_history(&[], || {
            let mut mock = MockTmux::default();
            mock.current_session = "s".to_string();
            mock.windows = vec![win("s", 1), win("s", 2)];

            mock.current_window = ("s".to_string(), 1);
            last_window(false).run(&mock).unwrap();
            assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,2)");

            tick();
            mock.current_window = ("s".to_string(), 2);
            last_window(false).run(&mock).unwrap();
            assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,1)");

            tick();
            mock.current_window = ("s".to_string(), 1);
            last_window(false).run(&mock).unwrap();
            assert_eq!(mock.calls().last().unwrap(), "switch_to_window(s,2)");
        });
    }

    /// Creating a new window must not hijack an already-established toggle:
    /// with s:1 and s:2 in a stable ping-pong, a fresh background window s:3
    /// (no history) is ignored and last-window still returns to s:2.
    #[test]
    fn a_new_window_does_not_disrupt_an_established_ping_pong() {
        let mut mock = MockTmux::default();
        mock.current_session = "s".to_string();
        mock.current_window = ("s".to_string(), 1);
        mock.windows = vec![win("s", 1), win("s", 2), win("s", 3)];

        // s:2 was visited most recently before the current window; s:3 is new.
        with_history(&[("s", 1, 900), ("s", 2, 1000)], || {
            last_window(false).run(&mock).unwrap();
        });

        assert!(mock.called("switch_to_window(s,2)"));
        assert!(!mock.called("switch_to_window(s,3)"));
    }
}
