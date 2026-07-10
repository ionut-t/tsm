use crate::cli::utils::PREVIEW_CMD;
use crate::error::Result;
use crate::history::WindowHistory;
use crate::history::paths;
use crate::{fzf::FzfPicker, tmux::TmuxClient};

// ANSI styling for the picker rows (fzf is launched with `--ansi`).
const POSITION_COLOR: &str = "\x1b[35m"; // magenta
const SESSION_COLOR: &str = "\x1b[36m"; // cyan
const SEPARATOR_COLOR: &str = "\x1b[90m"; // bright black
const INDEX_COLOR: &str = "\x1b[2m"; // dim
const RESET: &str = "\x1b[0m";

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
        let windows = client.list_windows()?;

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

        // Align the session and window-name columns so rows can be scanned vertically.
        let session_width = windows
            .iter()
            .map(|w| w.session_name.chars().count())
            .max()
            .unwrap_or(0);
        let name_width = windows
            .iter()
            .map(|w| w.name.chars().count())
            .max()
            .unwrap_or(0);
        let position_width = windows.len().to_string().len();

        // Each row is three tab-delimited fields:
        //
        //   1: pane_id  — hidden (`--with-nth 2..`); identifies the window on the way
        //                 back and feeds the preview.
        //   2: label    — shown and searched (`--nth 1` over the displayed fields):
        //                 the 1-based position plus session and window name.
        //   3: [index]  — shown but NOT searched, so its digits can never collide with a
        //                 position query. The position is unique, so typing a number
        //                 jumps straight to one row.
        //
        // The label is padded to a fixed visible width, so the tab before the index
        // column lands on the same tab stop for every row and the indexes line up.
        let items = windows
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let position = i + 1;
                format!(
                    "{pane}\t{poc}{position:>pw$}{reset} {sc}{session:<sw$}{reset} {pc}→{reset} {name:<nw$}{reset}\t{ic}[{index}]{reset}",
                    pane = w.pane_id,
                    poc = POSITION_COLOR,
                    position = position,
                    pw = position_width,
                    sc = SESSION_COLOR,
                    session = w.session_name,
                    sw = session_width,
                    pc = SEPARATOR_COLOR,
                    name = w.name,
                    nw = name_width,
                    ic = INDEX_COLOR,
                    index = w.index,
                    reset = RESET,
                )
            })
            .collect::<Vec<String>>();

        let preview_cmd = if self.preview { PREVIEW_CMD } else { "" };

        let picker = FzfPicker::new()
            .with_prompt(&self.prompt)
            .with_preview_command(preview_cmd)
            .with_delimiter("\t")
            .with_nth("2..")
            .with_search_nth("1");

        let selection = match picker.pick(&items)? {
            Some(sel) => sel,
            None => return Ok(()), // User canceled
        };

        let pane_id = selection.split('\t').next().ok_or_else(|| {
            crate::error::TsmError::InvalidArgument(
                "Failed to parse fzf selection for pane id".to_string(),
            )
        })?;

        let window = windows.iter().find(|w| w.pane_id == pane_id).ok_or_else(|| {
            crate::error::TsmError::InvalidArgument(format!(
                "Selected window with pane id {} not found",
                pane_id
            ))
        })?;

        history.record_access(&window.session_name, window.index)?;
        history.save()?;

        if client.is_inside_tmux() {
            client.switch_to_window(&window.session_name, window.index)?;
        } else {
            client.attach_to_window(&window.session_name, window.index)?;
        }

        Ok(())
    }
}
