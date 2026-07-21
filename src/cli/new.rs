use std::path::Path;

use crate::cli::utils::PREVIEW_LS_TREE_CMD;
use crate::error::Result;
use crate::fzf::{Picker, PickerOptions};
use crate::tmux::Tmux;
use crate::zoxide::DirectorySource;

/// Creates a new tmux session with optional directory selection via zoxide and fzf.
///
/// If a session with the specified name already exists, switches to it instead of creating a new one.
#[derive(clap::Parser, Debug)]
pub struct NewCommand {
    /// Session name (auto-generated if not provided)
    #[clap(short, long)]
    name: Option<String>,

    /// Directory path (skips zoxide if provided)
    #[clap(short, long)]
    path: Option<String>,

    /// Show directory preview in fzf
    #[clap(short = 'v', long, default_value_t = false)]
    preview: bool,

    /// fzf prompt
    #[clap(short = 'P', long, default_value = "Select directory: ")]
    prompt: String,

    /// No success message
    #[clap(short = 'q', long, default_value_t = false)]
    quiet: bool,
}

impl NewCommand {
    /// Executes the new session command.
    ///
    /// Creates a new tmux session or switches to an existing one with the same name.
    /// If no path is provided, prompts the user to select a directory using zoxide and fzf.
    pub fn run(
        &self,
        client: &dyn Tmux,
        picker: &dyn Picker,
        directories: &dyn DirectorySource,
    ) -> Result<()> {
        let path = if let Some(p) = self.path.clone() {
            p
        } else {
            let dirs = directories.query_directories()?;

            let preview_cmd = if self.preview {
                PREVIEW_LS_TREE_CMD
            } else {
                ""
            };

            let options = PickerOptions::new()
                .with_prompt(&self.prompt)
                .with_preview_command(preview_cmd);

            match picker.pick(&options, &dirs)? {
                Some(selection) => selection,
                None => return Ok(()),
            }
        };

        let expanded_path = if path.starts_with('~') {
            let home = std::env::home_dir().ok_or(crate::error::TsmError::HomeDirectoryNotFound)?;
            path.replacen('~', &home.to_string_lossy(), 1)
        } else if path == "." {
            std::env::current_dir()?.to_string_lossy().to_string()
        } else {
            path
        };

        let name = if let Some(n) = self.name.clone() {
            sanitise_session_name(&n)
        } else {
            match Path::new(&expanded_path).file_name() {
                Some(os_str) => sanitise_session_name(&os_str.to_string_lossy()),
                None => "_".to_string(),
            }
        };

        let sessions = client.list_sessions()?;
        if sessions.contains(&name) {
            if client.is_inside_tmux() {
                client.switch_session(&name)?;
            } else {
                client.attach_session(&name)?;
            }

            if !self.quiet {
                client.display_message(&format!(
                    "{} session already exists. Switching to it.",
                    name
                ))?;
            }
            return Ok(());
        }

        client.new_session(name.clone(), expanded_path)?;

        if !self.quiet {
            client.display_message(&format!("Created new session '{}'", name))?;
        }
        Ok(())
    }
}

fn sanitise_session_name(name: &str) -> String {
    let mut name = name;

    if name.starts_with('.') {
        name = name.trim_start_matches(".");
    }

    let sanitised: String = name
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == '.' {
                '_'
            } else {
                c
            }
        })
        .collect();

    if sanitised.is_empty() {
        "_".to_string()
    } else {
        sanitised
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockDirectories, MockPicker, MockTmux};

    fn cmd(name: Option<&str>, path: Option<&str>, quiet: bool) -> NewCommand {
        NewCommand {
            name: name.map(String::from),
            path: path.map(String::from),
            preview: false,
            prompt: "Select directory: ".to_string(),
            quiet,
        }
    }

    #[test]
    fn creates_session_with_explicit_name_and_path() {
        let mock = MockTmux::default();
        cmd(Some("dev"), Some("/tmp/project"), false)
            .run(
                &mock,
                &MockPicker::cancelling(),
                &MockDirectories::failing(),
            )
            .unwrap();
        assert!(mock.called("new_session(dev,/tmp/project)"));
        assert!(mock.called("display_message(Created new session 'dev')"));
    }

    #[test]
    fn derives_session_name_from_path_basename() {
        let mock = MockTmux::default();
        cmd(None, Some("/tmp/my-project"), true)
            .run(
                &mock,
                &MockPicker::cancelling(),
                &MockDirectories::failing(),
            )
            .unwrap();
        assert!(mock.called("new_session(my-project,/tmp/my-project)"));
    }

    #[test]
    fn sanitises_derived_name_with_dots() {
        let mock = MockTmux::default();
        cmd(None, Some("/tmp/.config"), true)
            .run(
                &mock,
                &MockPicker::cancelling(),
                &MockDirectories::failing(),
            )
            .unwrap();
        // Leading dot stripped by sanitise_session_name.
        assert!(mock.called("new_session(config,/tmp/.config)"));
    }

    #[test]
    fn switches_to_existing_session_instead_of_creating() {
        let mut mock = MockTmux::default();
        mock.sessions = vec!["dev".to_string()];
        cmd(Some("dev"), Some("/tmp/project"), false)
            .run(
                &mock,
                &MockPicker::cancelling(),
                &MockDirectories::failing(),
            )
            .unwrap();
        assert!(mock.called("switch_session(dev)"));
        assert!(!mock.called("new_session"));
        assert!(mock.called("display_message(dev session already exists"));
    }

    #[test]
    fn attaches_to_existing_session_when_outside_tmux() {
        let mut mock = MockTmux::default();
        mock.inside_tmux = false;
        mock.sessions = vec!["dev".to_string()];
        cmd(Some("dev"), Some("/tmp/project"), true)
            .run(
                &mock,
                &MockPicker::cancelling(),
                &MockDirectories::failing(),
            )
            .unwrap();
        assert!(mock.called("attach_session(dev)"));
        assert!(!mock.called("switch_session"));
    }

    #[test]
    fn picks_a_directory_when_no_path_given() {
        let mock = MockTmux::default();
        let dirs = MockDirectories::with(&["/tmp/alpha", "/tmp/beta"]);
        let picker = MockPicker::returning("/tmp/beta");

        cmd(None, None, false).run(&mock, &picker, &dirs).unwrap();

        // The picker is offered the zoxide list; the pick becomes the session,
        // named after the directory's basename.
        assert_eq!(
            picker.shown(),
            vec![vec!["/tmp/alpha".to_string(), "/tmp/beta".to_string()]]
        );
        assert!(mock.called("new_session(beta,/tmp/beta)"));
    }

    #[test]
    fn explicit_name_overrides_picked_directory_basename() {
        let mock = MockTmux::default();
        let dirs = MockDirectories::with(&["/tmp/alpha"]);
        cmd(Some("custom"), None, true)
            .run(&mock, &MockPicker::returning("/tmp/alpha"), &dirs)
            .unwrap();
        assert!(mock.called("new_session(custom,/tmp/alpha)"));
    }

    #[test]
    fn cancelling_the_directory_picker_creates_nothing() {
        let mock = MockTmux::default();
        let dirs = MockDirectories::with(&["/tmp/alpha"]);
        cmd(None, None, false)
            .run(&mock, &MockPicker::cancelling(), &dirs)
            .unwrap();
        assert!(!mock.called("new_session"));
    }

    #[test]
    fn propagates_a_zoxide_failure() {
        let mock = MockTmux::default();
        let err = cmd(None, None, false)
            .run(
                &mock,
                &MockPicker::cancelling(),
                &MockDirectories::failing(),
            )
            .unwrap_err();
        assert!(matches!(err, crate::error::TsmError::ZoxideQueryFailed));
    }

    #[test]
    fn keeps_plain_names_unchanged() {
        assert_eq!(sanitise_session_name("myproject"), "myproject");
        assert_eq!(sanitise_session_name("api-server_2"), "api-server_2");
    }

    #[test]
    fn replaces_whitespace_with_underscore() {
        assert_eq!(sanitise_session_name("my project"), "my_project");
        assert_eq!(sanitise_session_name("a\tb\nc"), "a_b_c");
    }

    #[test]
    fn replaces_interior_dots_with_underscore() {
        assert_eq!(sanitise_session_name("v1.2.3"), "v1_2_3");
    }

    #[test]
    fn strips_leading_dots_then_sanitises() {
        // A leading-dot directory like ".config" becomes "config".
        assert_eq!(sanitise_session_name(".config"), "config");
        // Only the leading run of dots is trimmed; interior dots still convert.
        assert_eq!(sanitise_session_name("..a.b"), "a_b");
    }

    #[test]
    fn empty_and_dot_only_names_fall_back_to_underscore() {
        assert_eq!(sanitise_session_name(""), "_");
        assert_eq!(sanitise_session_name("..."), "_");
        assert_eq!(sanitise_session_name("   "), "___");
    }

    #[test]
    fn preserves_unicode_word_characters() {
        assert_eq!(sanitise_session_name("café"), "café");
    }
}
