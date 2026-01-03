use crate::error::Result;
use crate::tmux::TmuxClient;

/// Renames a tmux session.
///
/// Renames the current session if no current name is specified.
#[derive(clap::Parser, Debug)]
pub struct RenameCommand {
    /// Current name - defaults to the active session if not provided
    #[clap(short = 'c', long)]
    current_name: Option<String>,
    /// New name
    #[clap(short = 'n', long)]
    new_name: String,
}

impl RenameCommand {
    /// Executes the rename session command.
    pub fn run(&self, client: &TmuxClient) -> Result<()> {
        client.rename_session(self.current_name.as_deref(), &self.new_name)?;
        Ok(())
    }
}
