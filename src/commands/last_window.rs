use crate::error::Result;
use crate::history::WindowHistory;
use crate::tmux::TmuxClient;

pub fn handle(client: &TmuxClient) -> Result<()> {
    let windows = client.list_windows();

    if windows.is_empty() {
        eprintln!("No windows found");
        return Ok(());
    }

    let history_file = format!("{}/.tsm_history", std::env::var("HOME").unwrap_or_default());
    let mut history = WindowHistory::new(history_file);
    history.load()?;
    history.record_current_window(client)?;

    let mut indexed_windows: Vec<_> = windows
        .into_iter()
        .map(|w| {
            let last_access = history
                .get_last_access(&w.session_name, w.index)
                .unwrap_or(0);
            (w, last_access)
        })
        .collect();
    indexed_windows.sort_by(|a, b| b.1.cmp(&a.1));

    // Get the previous window (index 1 = second in sorted list, after current window)
    if let Some((window, _)) = indexed_windows.get(1) {
        history.record_access(&window.session_name, window.index);
        history.save()?;

        if client.is_inside_tmux() {
            client.switch_to_window(&window.session_name, window.index)?;
        } else {
            client.attach_to_window(&window.session_name, window.index)?;
        }
    } else {
        eprintln!("No previous window found");
    }

    Ok(())
}
