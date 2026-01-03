use crate::error::Result;
use crate::fzf::FzfPicker;
use crate::tmux::TmuxClient;

/// Switches to a tmux session by name or via interactive selection.
#[derive(clap::Parser, Debug)]
pub struct SwitchCommand {
    /// Name of the session to switch to
    #[clap(short, long)]
    name: Option<String>,

    /// fzf prompt
    #[clap(short = 'P', long, default_value = "Select: ")]
    prompt: String,
}

impl SwitchCommand {
    /// Executes the switch session command.
    ///
    /// Switches to the specified session or prompts for selection via fzf if no name is provided.
    pub fn run(&self, client: &TmuxClient) -> Result<()> {
        let target = match self.name.clone() {
            Some(n) => n,
            None => {
                let picker = FzfPicker::new().with_prompt(&self.prompt);
                let sessions = client.list_sessions();
                match picker.pick(&sessions)? {
                    Some(selection) => selection,
                    None => return Ok(()),
                }
            }
        };

        if client.is_inside_tmux() {
            client.switch_session(&target)?;
        } else {
            client.attach_session(&target)?;
        }
        Ok(())
    }
}
