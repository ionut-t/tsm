use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{
    error::{Result, TsmError},
    tmux::Tmux,
    workspace::config::{Pane, Workspace},
};

pub struct WorkspaceRunner<'a> {
    client: &'a dyn Tmux,
    workspace: Workspace,
    session_name: String,
    root_path: Option<PathBuf>,
}

impl<'a> WorkspaceRunner<'a> {
    pub fn new(
        client: &'a dyn Tmux,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::config::Pane;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn merge_env_overlays_overrides_onto_base() {
        let base = map(&[("A", "1"), ("B", "2")]);
        let overrides = map(&[("B", "20"), ("C", "3")]);
        let merged = merge_env(&base, &overrides);

        assert_eq!(merged.get("A").map(String::as_str), Some("1"));
        assert_eq!(
            merged.get("B").map(String::as_str),
            Some("20"),
            "override wins"
        );
        assert_eq!(merged.get("C").map(String::as_str), Some("3"));
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn merge_env_does_not_mutate_base() {
        let base = map(&[("A", "1")]);
        let _ = merge_env(&base, &map(&[("A", "2")]));
        assert_eq!(base.get("A").map(String::as_str), Some("1"));
    }

    fn pane_with_env(pairs: &[(&str, &str)]) -> Pane {
        Pane {
            command: None,
            width: None,
            focus: false,
            env: map(pairs),
        }
    }

    #[test]
    fn pane_env_without_pane_clones_window_env() {
        let window_env = map(&[("A", "1")]);
        assert_eq!(pane_env(&window_env, None), window_env);
    }

    #[test]
    fn pane_env_with_empty_pane_env_clones_window_env() {
        let window_env = map(&[("A", "1")]);
        let pane = pane_with_env(&[]);
        assert_eq!(pane_env(&window_env, Some(&pane)), window_env);
    }

    #[test]
    fn pane_env_overlays_pane_overrides() {
        let window_env = map(&[("A", "1"), ("B", "2")]);
        let pane = pane_with_env(&[("B", "22"), ("C", "3")]);
        let merged = pane_env(&window_env, Some(&pane));

        assert_eq!(merged.get("A").map(String::as_str), Some("1"));
        assert_eq!(merged.get("B").map(String::as_str), Some("22"));
        assert_eq!(merged.get("C").map(String::as_str), Some("3"));
    }

    #[test]
    fn expand_tilde_expands_leading_tilde() {
        let home = dirs::home_dir().expect("home dir available in test env");
        let expanded = expand_tilde("~/projects/app");
        assert_eq!(expanded, home.join("projects/app"));
    }

    #[test]
    fn expand_tilde_leaves_absolute_and_relative_paths_untouched() {
        assert_eq!(expand_tilde("/etc/hosts"), PathBuf::from("/etc/hosts"));
        assert_eq!(expand_tilde("relative/dir"), PathBuf::from("relative/dir"));
        // A tilde that isn't the first character is not expanded.
        assert_eq!(expand_tilde("/a/~/b"), PathBuf::from("/a/~/b"));
    }

    // --- WorkspaceRunner::run layout engine -------------------------------

    use crate::test_support::MockTmux;

    fn run_workspace(toml: &str, mock: &MockTmux) {
        let ws: Workspace = toml::from_str(toml).unwrap();
        // Explicit root path avoids depending on the process working directory.
        WorkspaceRunner::new(mock, ws, None, Some(PathBuf::from("/tmp/root")))
            .run()
            .unwrap();
    }

    #[test]
    fn empty_workspace_creates_session_and_switches_when_inside_tmux() {
        let mock = MockTmux::default();
        run_workspace(r#"name = "proj""#, &mock);
        assert_eq!(
            mock.calls(),
            vec![
                "create_session_detached(proj)".to_string(),
                "switch_session(proj)".to_string(),
            ]
        );
    }

    #[test]
    fn empty_workspace_attaches_when_outside_tmux() {
        let mut mock = MockTmux::default();
        mock.inside_tmux = false;
        run_workspace(r#"name = "proj""#, &mock);
        assert_eq!(
            mock.calls(),
            vec![
                "create_session_detached(proj)".to_string(),
                "attach_session(proj)".to_string(),
            ]
        );
    }

    #[test]
    fn session_name_override_is_used() {
        let mock = MockTmux::default();
        let ws: Workspace = toml::from_str(r#"name = "proj""#).unwrap();
        WorkspaceRunner::new(
            &mock,
            ws,
            Some("custom".to_string()),
            Some(PathBuf::from("/tmp")),
        )
        .run()
        .unwrap();
        assert!(mock.called("create_session_detached(custom)"));
        assert!(mock.called("switch_session(custom)"));
    }

    #[test]
    fn builds_full_window_layout_in_order() {
        // One window, two rows; the first row has two panes, the second row a
        // single pane with a fixed height. Verifies the exact tmux call
        // sequence and — via the mock's distinct split ids — which pane each
        // operation targets.
        let mock = MockTmux::default();
        run_workspace(
            r#"
                name = "dev"
                [[window]]
                name = "main"
                [[window.row]]
                [[window.row.pane]]
                command = "a"
                [[window.row.pane]]
                command = "b"
                [[window.row]]
                height = 40
                [[window.row.pane]]
                command = "c"
            "#,
            &mock,
        );

        assert_eq!(
            mock.calls(),
            vec![
                "create_session_detached(dev)",
                "rename_window(dev,main)",
                // Row 2 is split off row 1's first pane (%0), yielding %p1.
                "split_vertical(%0->%p1)",
                // The fixed-height row is resized by its first pane (%p1).
                "resize_pane_height(%p1,40)",
                // Row 1's second pane splits horizontally off %0, yielding %p2.
                "split_horizontal(%0->%p2)",
                // Commands land in their panes: a→%0, b→%p2 (row 1), c→%p1 (row 2).
                "send_keys(%0,a)",
                "send_keys(%p2,b)",
                "send_keys(%p1,c)",
                "switch_session(dev)",
            ]
        );
    }

    #[test]
    fn respawns_first_pane_only_when_env_differs_from_session_env() {
        // A window-level env override means the first pane (spawned by
        // new-session with only the workspace env) must be respawned with the
        // effective env.
        let mock = MockTmux::default();
        run_workspace(
            r#"
                name = "e"
                [[window]]
                [window.env]
                FOO = "bar"
                [[window.row]]
                [[window.row.pane]]
            "#,
            &mock,
        );
        assert!(mock.called("respawn_pane(%0)"));
    }

    #[test]
    fn does_not_respawn_first_pane_when_env_matches_session_env() {
        // No window/pane env beyond the workspace env → the initial shell is
        // already correct, so no respawn.
        let mock = MockTmux::default();
        run_workspace(
            r#"
                name = "e"
                [env]
                FOO = "bar"
                [[window]]
                [[window.row]]
                [[window.row.pane]]
            "#,
            &mock,
        );
        assert!(!mock.called("respawn_pane"));
    }

    #[test]
    fn applies_focus_to_window_and_pane() {
        let mock = MockTmux::default();
        run_workspace(
            r#"
                name = "f"
                [[window]]
                focus = true
                [[window.row]]
                [[window.row.pane]]
                focus = true
            "#,
            &mock,
        );
        assert!(mock.called("select_window(f,0)"));
        assert!(mock.called("select_pane(%0)"));
    }

    #[test]
    fn additional_windows_are_created_with_new_window() {
        let mock = MockTmux::default();
        run_workspace(
            r#"
                name = "m"
                [[window]]
                name = "one"
                [[window.row]]
                [[window.row.pane]]
                [[window]]
                name = "two"
                [[window.row]]
                [[window.row.pane]]
            "#,
            &mock,
        );
        // First window reuses the session's initial window (rename); the second
        // is created fresh.
        assert!(mock.called("rename_window(m,one)"));
        assert!(mock.called("new_window(m,two)"));
    }

    #[test]
    fn applies_pane_width_resizes() {
        let mock = MockTmux::default();
        run_workspace(
            r#"
                name = "w"
                [[window]]
                [[window.row]]
                [[window.row.pane]]
                width = 70
                [[window.row.pane]]
            "#,
            &mock,
        );
        // The first pane of the row has a width, resized by its id (%0).
        assert!(mock.called("resize_pane_width(%0,70)"));
    }
}
