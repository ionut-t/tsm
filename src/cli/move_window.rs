use crate::{
    cli::utils::{PREVIEW_CMD, sort_windows_by_history},
    error::{Result, TsmError},
    fzf::{Picker, PickerOptions},
    history::{WindowHistory, paths},
    tmux::Tmux,
};

/// Moves a window from one session to another.
///
/// Can move a specified window or prompt for selection via fzf.
/// If the source session is not specified and a target is, defaults to the current window.
#[derive(clap::Parser, Debug)]
pub struct MoveWindowCommand {
    /// From session name
    #[clap(short, long)]
    from: Option<String>,

    /// To session name
    #[clap(short, long)]
    to: Option<String>,

    /// No success message
    #[clap(short = 'q', long, default_value_t = false)]
    quiet: bool,
}

impl MoveWindowCommand {
    /// Executes the move window command.
    ///
    /// Moves the specified or selected window to the target session and switches to it.
    pub fn run(&self, client: &dyn Tmux, picker: &dyn Picker) -> Result<()> {
        if self.from.is_none() && !client.is_inside_tmux() {
            return Err(crate::error::TsmError::NotInTmux);
        }

        let sessions = client.list_sessions();

        if sessions.len() < 2 {
            return Err(crate::error::TsmError::InvalidArgument(
                "At least two sessions are required to move a window".to_string(),
            ));
        }

        let mut history = WindowHistory::new(paths::history_file_path());
        history.load()?;

        let window_address = if self.from.is_none() && self.to.is_some() {
            let current_window = client.get_current_window()?;
            Some(current_window)
        } else {
            let windows = client.list_windows()?;

            let indexed_windows = sort_windows_by_history(windows, &history);
            let window_items: Vec<String> = indexed_windows
                .iter()
                .map(|(w, _)| format!("{}\t {}:{}", w.pane_id, w.session_name, w.index))
                .collect();

            find_window_to_move(&window_items, &self.from, picker)?
        };

        if let Some((from_session, from_window_index)) = window_address {
            let sessions_items: Vec<String> = sessions
                .iter()
                .filter(|s| *s != &from_session)
                .map(|s| s.to_string())
                .collect();

            if sessions_items.is_empty() {
                return Err(TsmError::InvalidArgument(
                    "No target sessions available to move window to".to_string(),
                ));
            }

            let target_session = find_target_session(&sessions_items, &self.to, picker)?;

            if let Some(to_session) = target_session {
                let pane_id = client.get_pane_id(&from_session, from_window_index)?;

                if client.is_last_window_in_session(&from_session)? {
                    client.switch_session(&to_session)?;
                }

                client.move_window(&from_session, from_window_index, &to_session)?;

                let (session, new_window_index) = client.find_window_by_pane_id(&pane_id)?;

                if client.is_inside_tmux() {
                    client.switch_to_window(session.as_str(), new_window_index)?;
                } else {
                    client.attach_to_window(session.as_str(), new_window_index)?;
                }

                history.record_access(&session, new_window_index)?;
                history.save()?;

                if !self.quiet {
                    client.display_message(&format!(
                        "Moved window {}:{} to session {}:{}",
                        from_session, from_window_index, to_session, new_window_index
                    ))?;
                }
            } else if !self.quiet {
                client.display_message("No target session selected, aborting move")?;
            }

            return Ok(());
        }

        Ok(())
    }
}

fn find_window_to_move(
    items: &[String],
    from: &Option<String>,
    picker: &dyn Picker,
) -> Result<Option<(String, u32)>> {
    if let Some(window_spec) = from {
        return Ok(Some(parse_window_spec(window_spec)?));
    }

    let options = PickerOptions::new()
        .with_prompt("Select window to move: ")
        .with_preview_command(PREVIEW_CMD)
        .with_delimiter("\t")
        .with_nth("2..");

    match picker.pick(&options, items)? {
        Some(selection) => {
            let parts: Vec<&str> = selection.split('\t').collect();
            if parts.len() != 2 {
                return Ok(None);
            }
            let window_spec = parts[1].trim();
            Ok(Some(parse_window_spec(window_spec)?))
        }
        None => Ok(None),
    }
}

fn find_target_session(
    items: &[String],
    to: &Option<String>,
    picker: &dyn Picker,
) -> Result<Option<String>> {
    if let Some(session_spec) = to {
        return Ok(Some(session_spec.clone()));
    }

    let options = PickerOptions::new().with_prompt("Select target session: ");
    match picker.pick(&options, items)? {
        Some(selection) => Ok(Some(selection)),
        None => Ok(None),
    }
}

fn parse_window_spec(spec: &str) -> Result<(String, u32)> {
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() != 2 {
        return Err(crate::error::TsmError::InvalidArgument(format!(
            "Invalid format '{}'. Use 'session:index'",
            spec
        )));
    }

    let session = parts[0].to_string();
    if let Ok(window_index) = parts[1].parse::<u32>() {
        Ok((session, window_index))
    } else {
        Err(crate::error::TsmError::InvalidArgument(format!(
            "Invalid window index in '{}'",
            spec
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TsmError;
    use crate::test_support::{MockPicker, MockTmux, with_env};
    use crate::tmux::Window;
    use tempfile::TempDir;

    fn win(session: &str, index: u32, pane: &str) -> Window {
        Window {
            session_name: session.to_string(),
            index,
            name: format!("w{index}"),
            pane_id: pane.to_string(),
        }
    }

    fn cmd(from: Option<&str>, to: Option<&str>, quiet: bool) -> MoveWindowCommand {
        MoveWindowCommand {
            from: from.map(String::from),
            to: to.map(String::from),
            quiet,
        }
    }

    fn with_temp_history(f: impl FnOnce()) {
        let tmp = TempDir::new().unwrap();
        let hist = tmp.path().join("history");
        with_env(&[("TSM_HISTORY_FILE", hist.to_str())], f);
    }

    #[test]
    fn errors_with_fewer_than_two_sessions() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string()];
        with_temp_history(|| {
            let err = cmd(Some("dev:1"), Some("prod"), false)
                .run(&mock, &MockPicker::cancelling())
                .unwrap_err();
            assert!(matches!(err, TsmError::InvalidArgument(_)));
        });
    }

    #[test]
    fn moves_window_to_target_and_follows_it() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string(), "prod".to_string()];
        mock.pane_id = "%moved".to_string();
        // dev has 2 windows (so the source is not the last), and the pane lands
        // in prod after the move — find_window_by_pane_id resolves it there.
        mock.windows = vec![
            win("dev", 1, "%d1"),
            win("dev", 2, "%d2"),
            win("prod", 1, "%moved"),
        ];

        with_temp_history(|| {
            cmd(Some("dev:1"), Some("prod"), false)
                .run(&mock, &MockPicker::cancelling())
                .unwrap();
        });

        assert!(mock.called("move_window(dev:1->prod)"));
        assert!(mock.called("switch_to_window(prod,1)"));
        assert!(mock.called("display_message(Moved window dev:1 to session prod:1)"));
    }

    #[test]
    fn switches_away_before_moving_the_last_window_in_a_session() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string(), "prod".to_string()];
        mock.pane_id = "%moved".to_string();
        // dev has a single window, so moving it empties the session; the command
        // switches to the target first to avoid detaching.
        mock.windows = vec![win("dev", 1, "%moved"), win("prod", 5, "%moved")];

        with_temp_history(|| {
            cmd(Some("dev:1"), Some("prod"), true)
                .run(&mock, &MockPicker::cancelling())
                .unwrap();
        });

        // switch_session(prod) happens before move_window in the call log.
        let calls = mock.calls();
        let switch_pos = calls.iter().position(|c| c == "switch_session(prod)");
        let move_pos = calls.iter().position(|c| c == "move_window(dev:1->prod)");
        assert!(switch_pos.is_some(), "expected switch_session(prod)");
        assert!(switch_pos < move_pos, "switch must precede the move");
    }

    #[test]
    fn quiet_suppresses_success_message() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string(), "prod".to_string()];
        mock.pane_id = "%moved".to_string();
        mock.windows = vec![
            win("dev", 1, "%d1"),
            win("dev", 2, "%d2"),
            win("prod", 1, "%moved"),
        ];
        with_temp_history(|| {
            cmd(Some("dev:1"), Some("prod"), true)
                .run(&mock, &MockPicker::cancelling())
                .unwrap();
        });
        assert!(mock.called("move_window(dev:1->prod)"));
        assert!(!mock.called("display_message"));
    }

    #[test]
    fn moves_current_window_when_only_target_is_given() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string(), "prod".to_string()];
        mock.current_window = ("dev".to_string(), 3);
        mock.pane_id = "%moved".to_string();
        // dev keeps another window so the source is not the last one.
        mock.windows = vec![
            win("dev", 3, "%d3"),
            win("dev", 4, "%d4"),
            win("prod", 1, "%moved"),
        ];

        with_temp_history(|| {
            // from omitted, to given → the current window (dev:3) is moved.
            cmd(None, Some("prod"), false)
                .run(&mock, &MockPicker::cancelling())
                .unwrap();
        });

        assert!(mock.called("move_window(dev:3->prod)"));
        assert!(mock.called("switch_to_window(prod,1)"));
    }

    #[test]
    fn errors_when_from_omitted_outside_tmux() {
        let mut mock = MockTmux::default();
        mock.inside_tmux = false;
        mock.sessions = vec!["dev".to_string(), "prod".to_string()];
        with_temp_history(|| {
            let err = cmd(None, Some("prod"), false)
                .run(&mock, &MockPicker::cancelling())
                .unwrap_err();
            assert!(matches!(err, TsmError::NotInTmux));
        });
    }

    #[test]
    fn picks_window_and_target_when_neither_given() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string(), "prod".to_string()];
        mock.pane_id = "%moved".to_string();
        mock.windows = vec![
            win("dev", 1, "%d1"),
            win("dev", 2, "%d2"),
            win("prod", 1, "%moved"),
        ];
        // First pick selects the window row; the second selects the target session.
        let picker = MockPicker::scripted(vec![
            Some("%d1\t dev:1".to_string()),
            Some("prod".to_string()),
        ]);

        with_temp_history(|| {
            cmd(None, None, false).run(&mock, &picker).unwrap();
        });

        let shown = picker.shown();
        // The window picker lists one row per window...
        assert!(shown[0].iter().any(|row| row.contains("dev:1")));
        // ...and the session picker lists only sessions other than the source.
        assert_eq!(shown[1], vec!["prod".to_string()]);
        assert!(mock.called("move_window(dev:1->prod)"));
        assert!(mock.called("switch_to_window(prod,1)"));
    }

    #[test]
    fn cancelling_the_window_picker_moves_nothing() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string(), "prod".to_string()];
        mock.windows = vec![win("dev", 1, "%d1"), win("prod", 1, "%p1")];
        with_temp_history(|| {
            cmd(None, None, false)
                .run(&mock, &MockPicker::cancelling())
                .unwrap();
        });
        assert!(!mock.called("move_window"));
    }

    #[test]
    fn parses_valid_window_spec() {
        assert_eq!(parse_window_spec("dev:3").unwrap(), ("dev".to_string(), 3));
        assert_eq!(parse_window_spec("x:0").unwrap(), ("x".to_string(), 0));
    }

    #[test]
    fn rejects_spec_without_colon() {
        let err = parse_window_spec("dev").unwrap_err();
        assert!(matches!(err, TsmError::InvalidArgument(_)));
        assert!(err.to_string().contains("Use 'session:index'"));
    }

    #[test]
    fn rejects_spec_with_extra_colon() {
        // A session name containing a colon is ambiguous and rejected.
        assert!(matches!(
            parse_window_spec("a:b:3"),
            Err(TsmError::InvalidArgument(_))
        ));
    }

    #[test]
    fn rejects_non_numeric_index() {
        let err = parse_window_spec("dev:abc").unwrap_err();
        assert!(matches!(err, TsmError::InvalidArgument(_)));
        assert!(err.to_string().contains("Invalid window index"));
    }

    #[test]
    fn rejects_negative_index() {
        // u32 parse rejects the sign.
        assert!(matches!(
            parse_window_spec("dev:-1"),
            Err(TsmError::InvalidArgument(_))
        ));
    }

    #[test]
    fn find_window_uses_explicit_from_without_picker() {
        // When `from` is supplied the fzf picker is never consulted, so the
        // `items` slice is irrelevant.
        let result =
            find_window_to_move(&[], &Some("dev:2".to_string()), &MockPicker::cancelling())
                .unwrap();
        assert_eq!(result, Some(("dev".to_string(), 2)));
    }

    #[test]
    fn find_window_propagates_invalid_explicit_from() {
        assert!(
            find_window_to_move(&[], &Some("garbage".to_string()), &MockPicker::cancelling())
                .is_err()
        );
    }

    #[test]
    fn find_target_session_returns_explicit_to() {
        let result =
            find_target_session(&[], &Some("prod".to_string()), &MockPicker::cancelling()).unwrap();
        assert_eq!(result, Some("prod".to_string()));
    }
}
