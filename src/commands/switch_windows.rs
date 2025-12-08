use crate::error::Result;
use crate::history::WindowHistory;
use crate::{fzf::FzfPicker, tmux::TmuxClient};

pub fn handle(client: &TmuxClient) -> Result<()> {
    let windows = client.list_windows();

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
    let windows: Vec<_> = indexed_windows.into_iter().map(|(w, _)| w).collect();

    let items = windows
        .iter()
        .enumerate()
        .map(|(i, w)| {
            format!(
                "{}\t{} -> {}[{}] -> {}",
                w.pane_id, i, w.name, w.index, w.session_name
            )
        })
        .collect::<Vec<String>>();

    let preview_cmd = r#"
PANE_ID=$(echo {} | cut -f1)
tmux capture-pane -e -p -t "$PANE_ID" 2>/dev/null || echo "No preview available"
"#
    .trim();

    let picker = FzfPicker::new()
        .with_prompt("Select window: ")
        .with_preview_command(preview_cmd)
        .with_delimiter("\t")
        .with_nth("2..");
    match picker.pick(&items) {
        Ok(Some(selection)) => {
            let selection_idx = selection
                .split('\t')
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            if let Some(window) = windows
                .iter()
                .enumerate()
                .find(|(i, _)| i == &selection_idx)
                .map(|(_, w)| w)
            {
                history.record_access(&window.session_name, window.index);
                history.save()?;

                if client.is_inside_tmux() {
                    client.switch_to_window(&window.session_name, window.index)?;
                } else {
                    client.attach_to_window(&window.session_name, window.index)?;
                }
            }
        }
        Ok(None) => {}
        Err(e) => return Err(e),
    }

    Ok(())
}
