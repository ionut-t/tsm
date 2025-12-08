use crate::error::Result;
use crate::fzf::FzfPicker;
use crate::tmux::TmuxClient;

pub fn handle(client: &TmuxClient, session: Option<String>) -> Result<()> {
    let target = match session {
        Some(n) => n,
        None => {
            let picker = FzfPicker::new().with_prompt("Kill session: ");
            let sessions = client.list_sessions();
            match picker.pick(&sessions)? {
                Some(selection) => selection,
                None => return Ok(()),
            }
        }
    };

    client.kill_session(&target)?;
    Ok(())
}
