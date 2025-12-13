use crate::error::Result;
use crate::fzf::FzfPicker;
use crate::tmux::TmuxClient;

pub fn handle(
    client: &TmuxClient,
    session: Option<String>,
    prompt: String,
    all: bool,
    quiet: bool,
) -> Result<()> {
    if all {
        client.kill_all_sessions()?;
        return Ok(());
    }

    let target = match session {
        Some(n) => n,
        None => {
            let picker = FzfPicker::new().with_prompt(&prompt);
            let sessions = client.list_sessions();
            match picker.pick(&sessions)? {
                Some(selection) => selection,
                None => return Ok(()),
            }
        }
    };

    client.kill_session(&target)?;

    if !quiet {
        client.display_message(&format!("Killed session: {}", target))?;
    }

    Ok(())
}
