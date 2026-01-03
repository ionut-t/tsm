use crate::error::Result;
use crate::fzf::FzfPicker;
use crate::tmux::TmuxClient;

/// Kills one or more tmux sessions.
///
/// Can kill a specific session by name, prompt for selection via fzf, or kill all sessions.
#[derive(clap::Parser, Debug)]
pub struct KillCommand {
    /// Session name
    #[clap(short, long)]
    session: Option<String>,

    /// Kill all
    #[clap(short = 'a', long, default_value_t = false)]
    all: bool,

    /// fzf prompt
    #[clap(short = 'P', long, default_value = "Kill session: ")]
    prompt: String,

    /// No success message
    #[clap(short = 'q', long, default_value_t = false)]
    quiet: bool,
}

impl KillCommand {
    /// Executes the kill session command.
    ///
    /// Kills the specified session, prompts for selection if no session is specified,
    /// or kills all sessions if the `--all` flag is set.
    pub fn run(&self, client: &TmuxClient) -> Result<()> {
        if self.all {
            client.kill_all_sessions()?;
            return Ok(());
        }

        let target = match self.session.clone() {
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

        client.kill_session(&target)?;

        if !self.quiet {
            client.display_message(&format!("Killed session: {}", target))?;
        }

        Ok(())
    }
}
