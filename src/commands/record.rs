use crate::{error::Result, history::WindowHistory, paths, tmux::TmuxClient};

pub fn handle(client: &TmuxClient) -> Result<()> {
    if !client.is_inside_tmux() {
        return Ok(());
    }

    let (session, window) = client.get_current_window()?;

    let history_file = paths::history_file_path().to_string_lossy().to_string();
    let mut history = WindowHistory::new(history_file);
    history.load()?;
    history.record_access(&session, window);
    history.save()?;

    Ok(())
}
