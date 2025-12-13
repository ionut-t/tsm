use crate::{
    TmuxClient,
    commands::utils::{PREVIEW_CMD, sort_windows_by_history},
    error::Result,
    fzf::FzfPicker,
    history::WindowHistory,
    paths,
};

pub fn handle(client: &TmuxClient, from: Option<String>, to: Option<String>) -> Result<()> {
    if from.is_none() && !client.is_inside_tmux() {
        client
            .display_message("Error: Must be inside a tmux session to move the current window")?;
        return Ok(());
    }

    let sessions = client.list_sessions();

    if sessions.len() < 2 {
        client.display_message("At least two sessions are required to move a window")?;
        return Ok(());
    }

    let windows = client.list_windows();

    let history_file = paths::history_file_path().to_string_lossy().to_string();
    let mut history = WindowHistory::new(history_file);
    history.load()?;

    let indexed_windows = sort_windows_by_history(windows, &history);
    let window_items: Vec<String> = indexed_windows
        .iter()
        .map(|(w, _)| format!("{}\t {}:{}", w.pane_id, w.session_name, w.index))
        .collect();

    let window_address = find_window_to_move(&window_items, from)?;

    if let Some((from_session, from_window_index)) = window_address {
        let sessions_items = sessions
            .iter()
            .filter(|s| *s != &from_session)
            .map(|s| s.to_string())
            .collect::<Vec<String>>();

        let target_session = find_target_session(&sessions_items, to)?;

        if let Some(to_session) = target_session {
            let pane_id = client.get_pane_id(&from_session, from_window_index)?;

            if client.is_last_window_in_session(&from_session) {
                client.switch_session(&to_session)?;
            }

            client.move_window(&from_session, from_window_index, &to_session)?;

            let (session, new_window_index) = client.find_window_by_pane_id(&pane_id)?;

            if client.is_inside_tmux() {
                client.switch_to_window(session.as_str(), new_window_index)?;
            } else {
                client.attach_to_window(session.as_str(), new_window_index)?;
            }

            history.record_access(&session, new_window_index);
            history.save()?;

            client.display_message(&format!(
                "Moved window {}:{} to session {}:{}",
                from_session, from_window_index, to_session, new_window_index
            ))?;
        } else {
            client.display_message("No target session selected, aborting move")?;
        }

        return Ok(());
    }

    Ok(())
}

fn find_window_to_move(items: &[String], from: Option<String>) -> Result<Option<(String, u32)>> {
    if let Some(window_spec) = from {
        return Ok(Some(parse_window_spec(&window_spec)?));
    }

    let picker = FzfPicker::new()
        .with_prompt("Select window to move: ")
        .with_preview_command(PREVIEW_CMD)
        .with_delimiter("\t")
        .with_nth("2..");

    match picker.pick(items)? {
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

fn find_target_session(items: &[String], to: Option<String>) -> Result<Option<String>> {
    if let Some(session_spec) = to {
        return Ok(Some(session_spec));
    }

    let picker = FzfPicker::new().with_prompt("Select target session: ");
    match picker.pick(items)? {
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
