use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{
    error::{Result, TsmError},
    tmux::TmuxClient,
    workspace::config::{Pane, Workspace},
};

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

        let windows = &self.workspace.window;

        // Create the session with only the workspace-level env. `new-session -e`
        // sets the *session* environment, so it both seeds the first pane and is
        // inherited by any window/pane spawned later (including ones the user
        // opens manually). Window- and pane-level overrides are deliberately
        // kept out of here so they don't leak into that session environment.
        self.client
            .create_session_detached(&self.session_name, &path, &self.workspace.env)?;

        if windows.is_empty() {
            return self.attach_or_switch();
        }

        let mut focus_window: Option<usize> = None;
        let mut focus_pane: Option<String> = None;

        for (i, window) in windows.iter().enumerate() {
            let window_index: usize;

            // Effective env for this window's panes: workspace env overlaid with
            // window-level overrides. Pane-level overrides are merged on top per
            // pane when each pane is spawned.
            let window_env = merge_env(&self.workspace.env, &window.env);
            let first_pane_env =
                pane_env(&window_env, window.row.first().and_then(|r| r.pane.first()));

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
                    &first_pane_env,
                )?;
            }

            if window.focus {
                focus_window = Some(window_index);
            }

            let initial_panes = self.client.list_panes(&self.session_name, window_index)?;
            let first_pane = initial_panes
                .first()
                .ok_or_else(|| {
                    TsmError::TmuxCommand(format!("No panes found in window {}", window_index))
                })?
                .clone();

            // The first window's initial pane was spawned by `new-session` with
            // only the workspace env. If this window or its first pane add any
            // overrides, respawn that idle shell with the full effective env so
            // it matches the others — without polluting the session environment.
            if i == 0 && first_pane_env != self.workspace.env {
                self.client
                    .respawn_pane(&first_pane, &path, &first_pane_env)?;
            }

            let mut row_first_panes: Vec<String> = vec![first_pane];

            for row_idx in 1..window.row.len() {
                // Split from the previous row's first pane to create the next row below
                let split_from = &row_first_panes[row_idx - 1];
                let row_pane_env = pane_env(&window_env, window.row[row_idx].pane.first());
                let new_row_pane =
                    self.client
                        .split_vertical(split_from, Some(&path), None, &row_pane_env)?;
                row_first_panes.push(new_row_pane);
            }

            // Resize rows that have a specified height
            for (row_idx, row) in window.row.iter().enumerate() {
                if let Some(height) = row.height {
                    self.client
                        .resize_pane_height(&row_first_panes[row_idx], height)?;
                }
            }

            // Split each row horizontally for its panes
            for (row_idx, row) in window.row.iter().enumerate() {
                self.create_row_panes(
                    &row_first_panes[row_idx],
                    &row.pane,
                    &path,
                    &window_env,
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
        panes: &[Pane],
        path: &Path,
        window_env: &HashMap<String, String>,
        focus_pane: &mut Option<String>,
    ) -> Result<()> {
        if panes.is_empty() {
            return Ok(());
        }

        let mut pane_ids = vec![first_pane_id.to_string()];

        // The row's first pane already exists with its env applied at creation.
        // Spawn the rest, each with its own effective env via tmux's -e flag.
        for pane in panes.iter().skip(1) {
            let new_pane_id = self.client.split_horizontal(
                &pane_ids[0],
                Some(path),
                None,
                &pane_env(window_env, Some(pane)),
            )?;
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

        // Send commands and track focus. Env is already set on each pane via -e.
        for (pane_idx, pane) in panes.iter().enumerate() {
            if let Some(pane_id) = pane_ids.get(pane_idx) {
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

/// Effective env for a single pane: the window-level env with the pane's own
/// overrides layered on top. Skips the merge (but still clones) when the pane
/// defines no env of its own.
fn pane_env(window_env: &HashMap<String, String>, pane: Option<&Pane>) -> HashMap<String, String> {
    match pane {
        Some(pane) if !pane.env.is_empty() => merge_env(window_env, &pane.env),
        _ => window_env.clone(),
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with('~')
        && let Some(home) = dirs::home_dir()
    {
        return PathBuf::from(path.replacen('~', &home.to_string_lossy(), 1));
    }

    PathBuf::from(path)
}
