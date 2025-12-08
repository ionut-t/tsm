use crate::error::Result;
use crate::tmux::TmuxClient;

pub fn handle(client: &TmuxClient, current_name: Option<String>, new_name: String) -> Result<()> {
    client.rename_session(current_name.as_deref(), &new_name)?;
    Ok(())
}
