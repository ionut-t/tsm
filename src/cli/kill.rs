use crate::error::Result;
use crate::fzf::{Picker, PickerOptions};
use crate::tmux::Tmux;

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
    pub fn run(&self, client: &dyn Tmux, picker: &dyn Picker) -> Result<()> {
        if self.all {
            client.kill_all_sessions()?;
            return Ok(());
        }

        let target = match self.session.clone() {
            Some(n) => n,
            None => {
                let options = PickerOptions::new().with_prompt(&self.prompt);
                let sessions = client.list_sessions();
                match picker.pick(&options, &sessions)? {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockPicker, MockTmux};

    fn cmd(session: Option<&str>, all: bool, quiet: bool) -> KillCommand {
        KillCommand {
            session: session.map(String::from),
            all,
            prompt: "Kill session: ".to_string(),
            quiet,
        }
    }

    #[test]
    fn all_flag_kills_server_without_naming_a_session() {
        let mock = MockTmux::default();
        cmd(None, true, false)
            .run(&mock, &MockPicker::cancelling())
            .unwrap();
        assert!(mock.called("kill_all_sessions()"));
        assert!(!mock.called("kill_session"));
    }

    #[test]
    fn kills_named_session_and_reports() {
        let mock = MockTmux::default();
        cmd(Some("dev"), false, false)
            .run(&mock, &MockPicker::cancelling())
            .unwrap();
        assert!(mock.called("kill_session(dev)"));
        assert!(mock.called("display_message(Killed session: dev)"));
    }

    #[test]
    fn quiet_suppresses_message() {
        let mock = MockTmux::default();
        cmd(Some("dev"), false, true)
            .run(&mock, &MockPicker::cancelling())
            .unwrap();
        assert!(mock.called("kill_session(dev)"));
        assert!(!mock.called("display_message"));
    }

    #[test]
    fn picks_a_session_when_none_given() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string(), "prod".to_string()];
        let picker = MockPicker::returning("prod");

        cmd(None, false, false).run(&mock, &picker).unwrap();

        // The picker is shown the current session list, and the selection is killed.
        assert_eq!(
            picker.shown(),
            vec![vec!["dev".to_string(), "prod".to_string()]]
        );
        assert!(mock.called("kill_session(prod)"));
        assert!(mock.called("display_message(Killed session: prod)"));
    }

    #[test]
    fn cancelling_the_picker_kills_nothing() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string()];
        cmd(None, false, false)
            .run(&mock, &MockPicker::cancelling())
            .unwrap();
        assert!(!mock.called("kill_session"));
    }
}
