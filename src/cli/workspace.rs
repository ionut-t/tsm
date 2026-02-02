use std::path::PathBuf;

use crate::error::{Result, TsmError};
use crate::fzf::FzfPicker;
use crate::tmux::TmuxClient;
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
    pub fn run(&self, client: &TmuxClient) -> Result<()> {
        match &self.subcommand {
            Some(WorkspaceSubcommand::List) => self.list_workspaces(),
            Some(WorkspaceSubcommand::Edit { name }) => self.edit_workspace(name),
            Some(WorkspaceSubcommand::New { name }) => self.create_workspace(name),
            Some(WorkspaceSubcommand::Path) => self.show_path(),
            Some(WorkspaceSubcommand::Delete { name }) => self.delete_workspace(name),
            None => self.launch_workspace(client),
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

    fn edit_workspace(&self, name: &Option<String>) -> Result<()> {
        let name = if let Some(n) = name {
            n.clone()
        } else {
            match pick_workspace("Select workspace to edit: ")? {
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

    fn delete_workspace(&self, name: &Option<String>) -> Result<()> {
        let name = if let Some(n) = name {
            n.clone()
        } else {
            match pick_workspace("Select workspace to delete: ")? {
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

    fn create_workspace(&self, name: &str) -> Result<()> {
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
        self.edit_workspace(&Some(name.to_string()))
    }

    fn show_path(&self) -> Result<()> {
        println!("{}", workspaces_dir().display());
        Ok(())
    }

    fn launch_workspace(&self, client: &TmuxClient) -> Result<()> {
        let name = if let Some(n) = &self.name {
            n.clone()
        } else {
            match pick_workspace("Select workspace to launch: ")? {
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

fn pick_workspace(prompt: &str) -> Result<Option<String>> {
    let workspaces = Workspace::list()?;
    if workspaces.is_empty() {
        return Err(TsmError::NoWorkspacesFound);
    }
    let dir = workspaces_dir();
    let preview_cmd = format!(
        "bat --color=always --style=plain {}/{{}}.toml",
        dir.display()
    );

    FzfPicker::new()
        .with_prompt(prompt)
        .with_preview_command(&preview_cmd)
        .pick(&workspaces)
}
