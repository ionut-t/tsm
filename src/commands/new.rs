use std::path::Path;

use crate::commands::utils::PREVIEW_CMD;
use crate::error::Result;
use crate::fzf::FzfPicker;
use crate::tmux::TmuxClient;
use crate::zoxide;

pub fn handle(
    client: &TmuxClient,
    name: Option<String>,
    path: Option<String>,
    preview: bool,
    prompt: String,
    quiet: bool,
) -> Result<()> {
    let path = if let Some(p) = path {
        p
    } else {
        let dirs = zoxide::query_directories()?;

        let preview_cmd = if preview { PREVIEW_CMD } else { "" };

        let picker = FzfPicker::new()
            .with_prompt(&prompt)
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

    let name = if let Some(n) = name {
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

        if !quiet {
            client.display_message(&format!(
                "{} session already exists. Switching to it.",
                name
            ))?;
        }
        return Ok(());
    }

    client.new_session(name.clone(), expanded_path)?;

    if !quiet {
        client.display_message(&format!("Created new session '{}'", name))?;
    }
    Ok(())
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
