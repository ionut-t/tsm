use crate::error::Result;
use crate::fzf::FzfPicker;
use crate::tmux::TmuxClient;

pub fn handle(client: &TmuxClient, name: Option<String>) -> Result<()> {
    let target = match name {
        Some(n) => n,
        None => {
            let picker = FzfPicker::new().with_prompt("Select session: ");
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
