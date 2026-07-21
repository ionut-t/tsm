use std::path::PathBuf;

use crate::cli::utils::shell_quote;
use crate::error::{Result, TsmError};
use crate::fzf::{Picker, PickerOptions};
use crate::tmux::Tmux;
use crate::workspace::config::Workspace;
use crate::workspace::paths::workspaces_dir;
use crate::workspace::runner::WorkspaceRunner;
use crate::workspace::template;

#[derive(clap::Parser, Debug)]
pub struct WorkspaceCommand {
    /// Workspace name (optional - shows picker if omitted)
    name: Option<String>,

    /// Override session name
    #[arg(short = 'n', long)]
    session_name: Option<String>,

    /// Override root directory
    #[arg(short, long)]
    path: Option<PathBuf>,

    #[command(subcommand)]
    subcommand: Option<WorkspaceSubcommand>,
}

#[derive(clap::Subcommand, Debug)]
enum WorkspaceSubcommand {
    /// List available workspaces
    List,
    /// Edit workspace file
    Edit { name: Option<String> },
    /// Create new workspace
    New { name: String },
    /// Delete workspace
    Delete { name: Option<String> },
    /// Show workspaces directory path
    Path,
}

impl WorkspaceCommand {
    pub fn run(&self, client: &dyn Tmux, picker: &dyn Picker) -> Result<()> {
        match &self.subcommand {
            Some(WorkspaceSubcommand::List) => self.list_workspaces(),
            Some(WorkspaceSubcommand::Edit { name }) => self.edit_workspace(name, picker),
            Some(WorkspaceSubcommand::New { name }) => self.create_workspace(name, picker),
            Some(WorkspaceSubcommand::Path) => self.show_path(),
            Some(WorkspaceSubcommand::Delete { name }) => self.delete_workspace(name, picker),
            None => self.launch_workspace(client, picker),
        }
    }

    fn list_workspaces(&self) -> Result<()> {
        let workspaces = Workspace::list()?;

        if workspaces.is_empty() {
            println!("No workspaces found in {}", workspaces_dir().display());
            return Ok(());
        }

        for name in workspaces {
            println!("{}", name);
        }
        Ok(())
    }

    fn edit_workspace(&self, name: &Option<String>, picker: &dyn Picker) -> Result<()> {
        let name = if let Some(n) = name {
            n.clone()
        } else {
            match pick_workspace("Select workspace to edit: ", picker)? {
                Some(selection) => selection,
                None => return Ok(()), // User canceled
            }
        };

        let path = workspaces_dir().join(format!("{}.toml", name));

        if !path.exists() {
            return Err(TsmError::WorkspaceNotFound(name));
        }

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

        std::process::Command::new(&editor)
            .arg(&path)
            .status()
            .map_err(TsmError::Io)?;

        Ok(())
    }

    fn delete_workspace(&self, name: &Option<String>, picker: &dyn Picker) -> Result<()> {
        let name = if let Some(n) = name {
            n.clone()
        } else {
            match pick_workspace("Select workspace to delete: ", picker)? {
                Some(selection) => selection,
                None => return Ok(()), // User canceled
            }
        };

        let path = workspaces_dir().join(format!("{}.toml", name));

        if !path.exists() {
            return Err(TsmError::WorkspaceNotFound(name));
        }

        std::fs::remove_file(&path)?;

        println!("Deleted workspace '{}'", name);

        Ok(())
    }

    fn create_workspace(&self, name: &str, picker: &dyn Picker) -> Result<()> {
        let dir = workspaces_dir();
        std::fs::create_dir_all(&dir)?;

        let path = dir.join(format!("{}.toml", name));

        if path.exists() {
            return Err(TsmError::WorkspaceAlreadyExists(name.to_string()));
        }

        let template = template::create_template(name);
        std::fs::write(&path, template)?;
        println!("Created workspace at {}", path.display());

        // Open in editor
        self.edit_workspace(&Some(name.to_string()), picker)
    }

    fn show_path(&self) -> Result<()> {
        println!("{}", workspaces_dir().display());
        Ok(())
    }

    fn launch_workspace(&self, client: &dyn Tmux, picker: &dyn Picker) -> Result<()> {
        let name = if let Some(n) = &self.name {
            n.clone()
        } else {
            match pick_workspace("Select workspace to launch: ", picker)? {
                Some(selection) => selection,
                None => return Ok(()), // User canceled
            }
        };

        let workspace = Workspace::load(&name)?;

        let runner = WorkspaceRunner::new(
            client,
            workspace,
            self.session_name.clone(),
            self.path.clone(),
        );

        runner.run()
    }
}

fn pick_workspace(prompt: &str, picker: &dyn Picker) -> Result<Option<String>> {
    let workspaces = Workspace::list()?;
    if workspaces.is_empty() {
        return Err(TsmError::NoWorkspacesFound);
    }
    let dir = workspaces_dir();
    let preview_cmd = format!(
        "bat --color=always --style=plain {}/{{}}.toml",
        shell_quote(&dir.display().to_string())
    );

    let options = PickerOptions::new()
        .with_prompt(prompt)
        .with_preview_command(&preview_cmd);
    picker.pick(&options, &workspaces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockPicker, MockTmux, with_env};
    use tempfile::TempDir;

    fn command(subcommand: WorkspaceSubcommand) -> WorkspaceCommand {
        WorkspaceCommand {
            name: None,
            session_name: None,
            path: None,
            subcommand: Some(subcommand),
        }
    }

    /// Run `f` with a temp config dir and a no-op `$EDITOR` (so the editor spawn
    /// in create/edit does nothing). Returns the workspaces directory.
    fn with_config_dir(f: impl FnOnce(&std::path::Path)) {
        let tmp = TempDir::new().unwrap();
        let ws_dir = tmp.path().join("workspaces");
        with_env(
            &[
                ("TSM_CONFIG_DIR", tmp.path().to_str()),
                ("XDG_CONFIG_HOME", None),
                ("EDITOR", Some("true")),
            ],
            || f(&ws_dir),
        );
    }

    #[test]
    fn create_writes_a_template_file() {
        let mock = MockTmux::default();
        with_config_dir(|ws_dir| {
            command(WorkspaceSubcommand::New {
                name: "fresh".to_string(),
            })
            .run(&mock, &MockPicker::cancelling())
            .unwrap();

            let path = ws_dir.join("fresh.toml");
            assert!(path.exists());
            let contents = std::fs::read_to_string(path).unwrap();
            assert!(contents.contains(r#"name = "fresh""#));
        });
    }

    #[test]
    fn create_rejects_an_existing_workspace() {
        let mock = MockTmux::default();
        with_config_dir(|ws_dir| {
            std::fs::create_dir_all(ws_dir).unwrap();
            std::fs::write(ws_dir.join("dup.toml"), r#"name = "dup""#).unwrap();

            let err = command(WorkspaceSubcommand::New {
                name: "dup".to_string(),
            })
            .run(&mock, &MockPicker::cancelling())
            .unwrap_err();
            assert!(matches!(err, TsmError::WorkspaceAlreadyExists(n) if n == "dup"));
        });
    }

    #[test]
    fn delete_removes_the_file() {
        let mock = MockTmux::default();
        with_config_dir(|ws_dir| {
            std::fs::create_dir_all(ws_dir).unwrap();
            let path = ws_dir.join("gone.toml");
            std::fs::write(&path, r#"name = "gone""#).unwrap();

            command(WorkspaceSubcommand::Delete {
                name: Some("gone".to_string()),
            })
            .run(&mock, &MockPicker::cancelling())
            .unwrap();
            assert!(!path.exists());
        });
    }

    #[test]
    fn delete_reports_missing_workspace() {
        let mock = MockTmux::default();
        with_config_dir(|_| {
            let err = command(WorkspaceSubcommand::Delete {
                name: Some("ghost".to_string()),
            })
            .run(&mock, &MockPicker::cancelling())
            .unwrap_err();
            assert!(matches!(err, TsmError::WorkspaceNotFound(n) if n == "ghost"));
        });
    }

    #[test]
    fn edit_reports_missing_workspace() {
        let mock = MockTmux::default();
        with_config_dir(|_| {
            let err = command(WorkspaceSubcommand::Edit {
                name: Some("ghost".to_string()),
            })
            .run(&mock, &MockPicker::cancelling())
            .unwrap_err();
            assert!(matches!(err, TsmError::WorkspaceNotFound(n) if n == "ghost"));
        });
    }

    #[test]
    fn launch_reports_missing_workspace() {
        let mock = MockTmux::default();
        with_config_dir(|_| {
            // A named workspace that doesn't exist fails to load rather than
            // silently creating anything.
            let cmd = WorkspaceCommand {
                name: Some("nope".to_string()),
                session_name: None,
                path: None,
                subcommand: None,
            };
            assert!(cmd.run(&mock, &MockPicker::cancelling()).is_err());
        });
    }

    #[test]
    fn list_and_path_subcommands_succeed() {
        let mock = MockTmux::default();
        with_config_dir(|ws_dir| {
            std::fs::create_dir_all(ws_dir).unwrap();
            std::fs::write(ws_dir.join("one.toml"), r#"name = "one""#).unwrap();
            // These print to stdout; assert they complete without error.
            command(WorkspaceSubcommand::List)
                .run(&mock, &MockPicker::cancelling())
                .unwrap();
            command(WorkspaceSubcommand::Path)
                .run(&mock, &MockPicker::cancelling())
                .unwrap();
        });
    }

    #[test]
    fn delete_picks_a_workspace_when_name_omitted() {
        let mock = MockTmux::default();
        with_config_dir(|ws_dir| {
            std::fs::create_dir_all(ws_dir).unwrap();
            std::fs::write(ws_dir.join("a.toml"), r#"name = "a""#).unwrap();
            std::fs::write(ws_dir.join("b.toml"), r#"name = "b""#).unwrap();

            let picker = MockPicker::returning("a");
            command(WorkspaceSubcommand::Delete { name: None })
                .run(&mock, &picker)
                .unwrap();

            // Only the picked workspace is removed.
            assert!(!ws_dir.join("a.toml").exists());
            assert!(ws_dir.join("b.toml").exists());
            // The picker was offered the available workspaces.
            assert_eq!(picker.shown().len(), 1);
            let mut shown = picker.shown()[0].clone();
            shown.sort();
            assert_eq!(shown, vec!["a".to_string(), "b".to_string()]);
        });
    }

    #[test]
    fn cancelling_the_delete_picker_removes_nothing() {
        let mock = MockTmux::default();
        with_config_dir(|ws_dir| {
            std::fs::create_dir_all(ws_dir).unwrap();
            std::fs::write(ws_dir.join("keep.toml"), r#"name = "keep""#).unwrap();

            command(WorkspaceSubcommand::Delete { name: None })
                .run(&mock, &MockPicker::cancelling())
                .unwrap();
            assert!(ws_dir.join("keep.toml").exists());
        });
    }

    #[test]
    fn picker_selection_errors_when_no_workspaces_exist() {
        let mock = MockTmux::default();
        with_config_dir(|_| {
            let err = command(WorkspaceSubcommand::Delete { name: None })
                .run(&mock, &MockPicker::cancelling())
                .unwrap_err();
            assert!(matches!(err, TsmError::NoWorkspacesFound));
        });
    }

    #[test]
    fn edit_picks_a_workspace_when_name_omitted() {
        let mock = MockTmux::default();
        with_config_dir(|ws_dir| {
            std::fs::create_dir_all(ws_dir).unwrap();
            std::fs::write(ws_dir.join("only.toml"), r#"name = "only""#).unwrap();

            // EDITOR=true makes the spawn a harmless no-op; the picked file
            // exists, so the edit flow completes.
            command(WorkspaceSubcommand::Edit { name: None })
                .run(&mock, &MockPicker::returning("only"))
                .unwrap();
            assert!(ws_dir.join("only.toml").exists());
        });
    }

    #[test]
    fn launch_picks_and_runs_the_workspace() {
        let mock = MockTmux::default();
        with_config_dir(|ws_dir| {
            std::fs::create_dir_all(ws_dir).unwrap();
            std::fs::write(ws_dir.join("dev.toml"), r#"name = "dev""#).unwrap();

            // No subcommand and no name → the launch flow selects via the picker.
            WorkspaceCommand {
                name: None,
                session_name: None,
                path: None,
                subcommand: None,
            }
            .run(&mock, &MockPicker::returning("dev"))
            .unwrap();

            assert!(mock.called("create_session_detached(dev)"));
        });
    }
}
