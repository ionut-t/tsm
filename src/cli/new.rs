use std::path::Path;

use crate::cli::utils::PREVIEW_CMD;
use crate::error::Result;
use crate::fzf::FzfPicker;
use crate::tmux::TmuxClient;
use crate::zoxide;

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
    pub fn run(&self, client: &TmuxClient) -> Result<()> {
        let path = if let Some(p) = self.path.clone() {
            p
        } else {
            let dirs = zoxide::query_directories()?;

            let preview_cmd = if self.preview { PREVIEW_CMD } else { "" };

            let picker = FzfPicker::new()
                .with_prompt(&self.prompt)
                .with_preview_command(preview_cmd);

            match picker.pick(&dirs)? {
                Some(selection) => selection,
                None => return Ok(()),
            }
        };

        let expanded_path = if path.starts_with('~') {
            std::env::home_dir()
                .map(|home| path.replacen('~', &home.to_string_lossy(), 1))
                .unwrap_or(path)
        } else if path == "." {
            std::env::current_dir()
                .map(|cwd| cwd.to_string_lossy().to_string())
                .unwrap_or(path)
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

        let sessions = client.list_sessions();
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

    name.chars()
        .map(|c| {
            if c.is_whitespace() || c == '.' {
                '_'
            } else {
                c
            }
        })
        .collect()
}
