use crate::cli::utils::PREVIEW_CMD;
use crate::error::Result;
use crate::history::WindowHistory;
use crate::paths;
use crate::{fzf::FzfPicker, tmux::TmuxClient};

/// Switches to a window via interactive selection.
///
/// Presents all windows across all sessions in an fzf picker, sorted by access history.
/// Optionally shows a preview of the window content.
#[derive(clap::Parser, Debug)]
pub struct SwitchWindowCommand {
    /// fzf prompt
    #[clap(short = 'P', long, default_value = "Select: ")]
    prompt: String,

    /// Show directory preview in fzf
    #[clap(short = 'v', long, default_value_t = false)]
    preview: bool,
}

impl SwitchWindowCommand {
    /// Executes the switch window command.
    ///
    /// Displays an fzf picker with all windows sorted by access history and switches to the selected window.
    pub fn run(&self, client: &TmuxClient) -> Result<()> {
        let windows = client.list_windows();

        let mut history = WindowHistory::new(paths::history_file_path());
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

        let preview_cmd = if self.preview { PREVIEW_CMD } else { "" };

        let picker = FzfPicker::new()
            .with_prompt(&self.prompt)
            .with_preview_command(preview_cmd)
            .with_delimiter("\t")
            .with_nth("2..");

        let selection = match picker.pick(&items)? {
            Some(sel) => sel,
            None => return Ok(()), // User canceled
        };

        let selection_idx = selection
            .split('\t')
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| {
                crate::error::TsmError::InvalidArgument(
                    "Failed to parse fzf selection for window index".to_string(),
                )
            })?;

        let window = windows.get(selection_idx).ok_or_else(|| {
            crate::error::TsmError::InvalidArgument(format!(
                "Selected window index {} not found",
                selection_idx
            ))
        })?;

        history.record_access(&window.session_name, window.index);
        history.save()?;

        if client.is_inside_tmux() {
            client.switch_to_window(&window.session_name, window.index)?;
        } else {
            client.attach_to_window(&window.session_name, window.index)?;
        }

        Ok(())
    }
}
