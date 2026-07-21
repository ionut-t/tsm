use crate::error::Result;
use crate::fzf::{Picker, PickerOptions};
use crate::tmux::Tmux;

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
    pub fn run(&self, client: &dyn Tmux, picker: &dyn Picker) -> Result<()> {
        let target = match self.name.clone() {
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

        if client.is_inside_tmux() {
            client.switch_session(&target)?;
        } else {
            client.attach_session(&target)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockPicker, MockTmux};

    fn cmd(name: Option<&str>) -> SwitchCommand {
        SwitchCommand {
            name: name.map(String::from),
            prompt: "Select: ".to_string(),
        }
    }

    #[test]
    fn switches_named_session_when_inside_tmux() {
        let mock = MockTmux::default();
        cmd(Some("dev"))
            .run(&mock, &MockPicker::cancelling())
            .unwrap();
        assert!(mock.called("switch_session(dev)"));
        assert!(!mock.called("attach_session"));
    }

    #[test]
    fn attaches_named_session_when_outside_tmux() {
        let mut mock = MockTmux::default();
        mock.inside_tmux = false;
        cmd(Some("dev"))
            .run(&mock, &MockPicker::cancelling())
            .unwrap();
        assert!(mock.called("attach_session(dev)"));
        assert!(!mock.called("switch_session"));
    }

    #[test]
    fn picks_a_session_when_no_name_given() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string(), "prod".to_string()];
        let picker = MockPicker::returning("prod");

        cmd(None).run(&mock, &picker).unwrap();

        assert_eq!(
            picker.shown(),
            vec![vec!["dev".to_string(), "prod".to_string()]]
        );
        assert!(mock.called("switch_session(prod)"));
    }

    #[test]
    fn cancelling_the_picker_switches_nothing() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string()];
        cmd(None).run(&mock, &MockPicker::cancelling()).unwrap();
        assert!(!mock.called("switch_session"));
        assert!(!mock.called("attach_session"));
    }
}
