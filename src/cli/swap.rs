use crate::error::Result;
use crate::error::TsmError;
use crate::history::WindowHistory;
use crate::paths;
use crate::tmux::TmuxClient;

/// Swaps the positions of two windows within the current session.
///
/// The source window defaults to the current window if not specified.
/// Both windows must be in the same session.
#[derive(clap::Parser, Debug)]
pub struct SwapWindowCommand {
    /// Source window index (defaults to the current window)
    #[clap(short, long)]
    source: Option<u32>,

    /// Target window index
    #[clap(short, long)]
    target: u32,

    /// No success message
    #[clap(short = 'q', long, default_value_t = false)]
    quiet: bool,
}

impl SwapWindowCommand {
    /// Executes the swap window command.
    ///
    /// Swaps the source and target windows and switches to the new position of the current window.
    pub fn run(&self, client: &TmuxClient) -> Result<()> {
        if !client.is_inside_tmux() {
            return Err(TsmError::NotInTmux);
        }

        let source_index = match self.source {
            Some(index) => index,
            None => {
                let (_, window_index) = client.get_current_window()?;
                window_index
            }
        };

        if source_index == self.target {
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

        if !session_windows.contains(&self.target) {
            client.display_message(&format!(
                "Window {} not found in current session",
                self.target
            ))?;
            return Ok(());
        }

        let (_, current_window_index) = client.get_current_window()?;

        client.swap_windows(source_index, self.target)?;

        if source_index == current_window_index {
            client.switch_to_window(&session, self.target)?;

            let mut history = WindowHistory::new(paths::history_file_path());
            history.load()?;
            history.record_access(&session, self.target);
            history.save()?;
        }

        if !self.quiet {
            client.display_message(&format!(
                "Swapped windows {} and {}",
                source_index, self.target,
            ))?;
        }

        Ok(())
    }
}
