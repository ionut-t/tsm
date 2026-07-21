use crate::error::Result;
use crate::tmux::Tmux;

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
    pub fn run(&self, client: &dyn Tmux) -> Result<()> {
        client.rename_session(self.current_name.as_deref(), &self.new_name)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockTmux;

    #[test]
    fn renames_explicit_current_name() {
        let mock = MockTmux::default();
        RenameCommand {
            current_name: Some("old".to_string()),
            new_name: "new".to_string(),
        }
        .run(&mock)
        .unwrap();
        assert!(mock.called("rename_session(old,new)"));
    }

    #[test]
    fn renames_active_session_when_current_name_omitted() {
        let mock = MockTmux::default();
        RenameCommand {
            current_name: None,
            new_name: "new".to_string(),
        }
        .run(&mock)
        .unwrap();
        // `-` marks "no explicit current name" — the client resolves the active one.
        assert!(mock.called("rename_session(-,new)"));
    }
}
