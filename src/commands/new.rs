use std::path::Path;

use crate::error::Result;
use crate::fzf::FzfPicker;
use crate::tmux::TmuxClient;
use crate::zoxide;

const PREVIEW_CMD: &str = r#"
DIR=$(echo {} | sed "s|^~|$HOME|")
eza --tree --level=2 --icons --group-directories-first --color=always "$DIR" 2>/dev/null || tree -C -L 2 "$DIR" 2>/dev/null || ls -lAhG "$DIR"
"#;

pub fn handle(
    client: &TmuxClient,
    name: Option<String>,
    path: Option<String>,
    preview: bool,
    prompt: String,
) -> Result<()> {
    let path = if let Some(p) = path {
        p
    } else {
        let dirs = zoxide::query_directories();

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

    client.new_session(name, expanded_path)?;
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
