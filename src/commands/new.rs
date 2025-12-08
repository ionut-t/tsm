use crate::error::Result;
use crate::tmux::TmuxClient;

pub fn handle(client: &TmuxClient, name: Option<String>, path: Option<String>) -> Result<()> {
    client.new_session(name.as_deref(), path.as_deref())
}
