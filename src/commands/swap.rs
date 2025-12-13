use crate::error::Result;
use crate::error::TsmError;
use crate::history::WindowHistory;
use crate::paths;
use crate::tmux::TmuxClient;

pub fn handle(client: &TmuxClient, source: Option<u32>, target: u32, quiet: bool) -> Result<()> {
    if !client.is_inside_tmux() {
        return Err(TsmError::NotInTmux);
    }

    let source_index = match source {
        Some(index) => index,
        None => {
            let (_, window_index) = client.get_current_window()?;
            window_index
        }
    };

    if source_index == target {
        client.display_message("Source and target window indices are the same")?;
        return Ok(());
    }

    let session = client.current_session()?;
    let all_windows = client.list_windows();
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

    if !session_windows.contains(&target) {
        client.display_message(&format!("Window {} not found in current session", target))?;
        return Ok(());
    }

    let (_, current_window_index) = client.get_current_window()?;

    client.swap_windows(source_index, target)?;

    if source_index == current_window_index {
        client.switch_to_window(&session, target)?;

        let history_file = paths::history_file_path().to_string_lossy().to_string();
        let mut history = WindowHistory::new(history_file);
        history.load()?;
        history.record_access(&session, target);
        history.save()?;
    }

    if !quiet {
        client.display_message(&format!("Swapped windows {} and {}", source_index, target,))?;
    }

    Ok(())
}
