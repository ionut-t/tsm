use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{error::Result, tmux::TmuxClient, workspace::config::Workspace};

pub struct WorkspaceRunner<'a> {
    client: &'a TmuxClient,
    workspace: Workspace,
    session_name: String,
    root_path: Option<PathBuf>,
}

impl<'a> WorkspaceRunner<'a> {
    pub fn new(
        client: &'a TmuxClient,
        workspace: Workspace,
        session_name: Option<String>,
        root_path: Option<PathBuf>,
    ) -> Self {
        let session_name = session_name.unwrap_or_else(|| workspace.name.clone());
        Self {
            client,
            workspace,
            session_name,
            root_path,
        }
    }

    pub fn run(&self) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let path = self
            .root_path
            .clone()
            .or_else(|| self.workspace.root.as_ref().map(|r| r.into()))
            .map(|p| expand_tilde(&p.to_string_lossy()))
            .unwrap_or(cwd);

        // Create session detached
        self.client
            .create_session_detached(&self.session_name, &path)?;

        let windows = &self.workspace.window;
        if windows.is_empty() {
            return self.attach_or_switch();
        }

        let mut focus_window: Option<usize> = None;
        let mut focus_pane: Option<String> = None;

        for (i, window) in windows.iter().enumerate() {
            let window_index: usize;

            if i == 0 {
                window_index = self.client.get_current_window_index(&self.session_name)?;
                if let Some(name) = &window.name {
                    self.client.rename_window(&self.session_name, name)?;
                }
            } else {
                window_index = self.client.new_window(
                    &self.session_name,
                    window.name.as_deref(),
                    Some(&path),
                )?;
            }

            if window.focus {
                focus_window = Some(window_index);
            }

            let initial_panes = self.client.list_panes(&self.session_name, window_index)?;
            let first_pane = initial_panes[0].clone();

            let mut row_first_panes: Vec<String> = vec![first_pane];

            for row_idx in 1..window.row.len() {
                // Split from the previous row's first pane to create the next row below
                let split_from = &row_first_panes[row_idx - 1];
                let new_row_pane = self.client.split_vertical(split_from, Some(&path), None)?;
                row_first_panes.push(new_row_pane);
            }

            // Resize rows that have a specified height
            for (row_idx, row) in window.row.iter().enumerate() {
                if let Some(height) = row.height {
                    self.client
                        .resize_pane_height(&row_first_panes[row_idx], height)?;
                }
            }

            let window_env: HashMap<String, String>;
            let effective_env = if window.env.is_empty() {
                &self.workspace.env
            } else {
                window_env = merge_env(&self.workspace.env, &window.env);
                &window_env
            };

            // Split each row horizontally for its panes
            for (row_idx, row) in window.row.iter().enumerate() {
                self.create_row_panes(
                    &row_first_panes[row_idx],
                    &row.pane,
                    &path,
                    effective_env,
                    &mut focus_pane,
                )?;
            }
        }

        if let Some(window_idx) = focus_window {
            self.client.select_window(&self.session_name, window_idx)?;
        }

        if let Some(pane_id) = focus_pane {
            self.client.select_pane(&pane_id)?;
        }

        self.attach_or_switch()
    }

    /// Create panes within a row by splitting horizontally
    fn create_row_panes(
        &self,
        first_pane_id: &str,
        panes: &[crate::workspace::config::Pane],
        path: &Path,
        inherited_env: &HashMap<String, String>,
        focus_pane: &mut Option<String>,
    ) -> Result<()> {
        if panes.is_empty() {
            return Ok(());
        }

        let mut pane_ids = vec![first_pane_id.to_string()];

        for _ in panes.iter().skip(1) {
            let new_pane_id = self
                .client
                .split_horizontal(&pane_ids[0], Some(path), None)?;
            pane_ids.push(new_pane_id);
        }

        // Resize panes that have a specified width
        for (pane_idx, pane) in panes.iter().enumerate() {
            if let Some(width) = pane.width
                && let Some(pane_id) = pane_ids.get(pane_idx)
            {
                self.client.resize_pane_width(pane_id, width)?;
            }
        }

        // Send env vars and commands, track focus
        for (pane_idx, pane) in panes.iter().enumerate() {
            if let Some(pane_id) = pane_ids.get(pane_idx) {
                let pane_env;
                let effective_env = if pane.env.is_empty() {
                    inherited_env
                } else {
                    pane_env = merge_env(inherited_env, &pane.env);
                    &pane_env
                };

                if !effective_env.is_empty() {
                    let export = build_export_command(effective_env);
                    self.client.send_keys(pane_id, &export)?;
                }

                if let Some(cmd) = &pane.command {
                    self.client.send_keys(pane_id, cmd)?;
                }
                if pane.focus {
                    *focus_pane = Some(pane_id.clone());
                }
            }
        }

        Ok(())
    }

    fn attach_or_switch(&self) -> Result<()> {
        if self.client.is_inside_tmux() {
            self.client.switch_session(&self.session_name)
        } else {
            self.client.attach_session(&self.session_name)
        }
    }
}

fn merge_env(
    base: &HashMap<String, String>,
    overrides: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = base.clone();
    merged.extend(overrides.iter().map(|(k, v)| (k.clone(), v.clone())));
    merged
}

fn build_export_command(env: &HashMap<String, String>) -> String {
    let mut parts: Vec<String> = env
        .iter()
        .map(|(k, v)| {
            let escaped = v.replace('\'', "'\\''");
            format!("{}='{}'", k, escaped)
        })
        .collect();
    parts.sort();
    format!("export {}", parts.join(" "))
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with('~')
        && let Some(home) = dirs::home_dir()
    {
        return PathBuf::from(path.replacen('~', &home.to_string_lossy(), 1));
    }

    PathBuf::from(path)
}
