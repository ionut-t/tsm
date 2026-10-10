use std::path::{Path, PathBuf};

use crate::{
    error::{Result, TsmError},
    tmux::Tmux,
    workspace::arrange::{self, Leaf},
    workspace::config::{Window, Workspace},
    workspace::layout,
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

        // Check the config before creating anything, so a mistake doesn't
        // leave a half-built session behind.
        for window in windows {
            arrange::validate(window)?;
        }

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

            // Every pane tmux will create for this window, in layout order,
            // each with its effective env (workspace, window, enclosing panes,
            // then its own).
            let leaves = arrange::leaves(window, &self.workspace.env);
            let first_pane_env = &leaves[0].env;

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
                    first_pane_env,
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
            if i == 0 && *first_pane_env != self.workspace.env {
                self.client
                    .respawn_pane(&first_pane, &path, first_pane_env)?;
            }

            self.create_panes(window, &leaves, first_pane, &path, &mut focus_pane)?;
        }

        if let Some(window_idx) = focus_window {
            self.client.select_window(&self.session_name, window_idx)?;
        }

        if let Some(pane_id) = focus_pane {
            self.client.select_pane(&pane_id)?;
        }

        self.attach_or_switch()
    }

    /// Create the rest of a window's panes, arrange them, then start their
    /// commands.
    ///
    /// tmux fills a layout with the window's panes in pane order, and each
    /// split here goes off the newest pane, so pane order is creation order:
    /// the same order as `leaves`.
    fn create_panes(
        &self,
        window: &Window,
        leaves: &[Leaf],
        first_pane_id: String,
        path: &Path,
        focus_pane: &mut Option<String>,
    ) -> Result<()> {
        let mut pane_ids = vec![first_pane_id];

        for leaf in leaves.iter().skip(1) {
            let last_pane_id = pane_ids.last().expect("pane_ids starts non-empty").clone();
            let new_pane_id = self
                .client
                .split_vertical(&last_pane_id, Some(path), &leaf.env)?;
            pane_ids.push(new_pane_id);
            // Re-tile after every split. Each split halves the newest pane, so
            // without this tmux runs out of room fast: on an 80x24 window the
            // 5th split fails with "no space for new pane". Creating every
            // pane first and applying the real layout once isn't possible for
            // that reason. Tiling only reshapes panes and never reorders them,
            // so pane order stays creation order. The cost is one extra tmux
            // call per pane (about 430 ms for 10 panes in total), and the
            // session is still detached here, so nothing flickers on screen.
            self.client.select_layout(&pane_ids[0], "tiled")?;
        }

        if leaves.len() > 1 {
            let (width, height) = self.client.window_size(&pane_ids[0])?;
            let layout = layout::render(&arrange::layout(window, width, height)?);
            self.client.select_layout(&pane_ids[0], &layout)?;
        }

        // Env is already set on each pane via -e.
        for (leaf, pane_id) in leaves.iter().zip(&pane_ids) {
            let Some(pane) = leaf.pane else { continue };
            if let Some(cmd) = &pane.command {
                self.client.send_keys(pane_id, cmd)?;
            }
            if pane.focus {
                *focus_pane = Some(pane_id.clone());
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
    fn builds_window_from_a_generated_layout() {
        // Two rows: two panes side by side, then one pane with a fixed
        // height. Every pane is created first, then one layout sized for the
        // window (120x40 in the mock) arranges them.
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
                // Each split goes off the newest pane, re-tiling for room.
                "split_vertical(%0->%p1)",
                "select_layout(%0,tiled)",
                "split_vertical(%p1->%p2)",
                "select_layout(%0,tiled)",
                "window_size(%0)",
                // Row 1 gets what row 2's 40% leaves; its panes split evenly.
                "select_layout(%0,116b,120x40,0,0[120x23,0,0{60x23,0,0,0,59x23,61,0,1},120x16,0,24,2])",
                // Commands follow creation order, which is layout order.
                "send_keys(%0,a)",
                "send_keys(%p1,b)",
                "send_keys(%p2,c)",
                "switch_session(dev)",
            ]
        );
    }

    #[test]
    fn panes_can_hold_rows_of_their_own() {
        // One tall pane on the left; the right half split into a top pane
        // and two bottom panes.
        let mock = MockTmux::default();
        run_workspace(
            r#"
                name = "n"
                [[window]]
                [[window.row]]
                [[window.row.pane]]
                command = "1"
                [[window.row.pane]]
                [[window.row.pane.row]]
                [[window.row.pane.row.pane]]
                command = "2"
                [[window.row.pane.row]]
                [[window.row.pane.row.pane]]
                command = "3"
                [[window.row.pane.row.pane]]
                command = "4"
                focus = true
            "#,
            &mock,
        );

        assert!(mock.called(
            "select_layout(%0,d099,120x40,0,0{60x40,0,0,0,59x40,61,0[59x20,61,0,1,59x19,61,21{29x19,61,21,2,29x19,91,21,3}]})"
        ));
        let sends: Vec<_> = mock
            .calls()
            .into_iter()
            .filter(|c| c.starts_with("send_keys") || c.starts_with("select_pane"))
            .collect();
        assert_eq!(
            sends,
            [
                "send_keys(%0,1)",
                "send_keys(%p1,2)",
                "send_keys(%p2,3)",
                "send_keys(%p3,4)",
                "select_pane(%p3)",
            ]
        );
    }

    #[test]
    fn single_pane_windows_skip_the_layout() {
        let mock = MockTmux::default();
        run_workspace(
            r#"
                name = "s"
                [[window]]
                [[window.row]]
                [[window.row.pane]]
                command = "a"
            "#,
            &mock,
        );
        assert!(!mock.called("select_layout"));
        assert!(!mock.called("window_size"));
        assert!(mock.called("send_keys(%0,a)"));
    }

    #[test]
    fn invalid_config_fails_before_creating_anything() {
        let mock = MockTmux::default();
        let ws: Workspace = toml::from_str(
            r#"
                name = "bad"
                [[window]]
                [[window.row]]
                [[window.row.pane]]
                command = "can't run in a container"
                [[window.row.pane.row]]
            "#,
        )
        .unwrap();

        let err = WorkspaceRunner::new(&mock, ws, None, Some(PathBuf::from("/tmp")))
            .run()
            .unwrap_err();
        assert!(matches!(err, TsmError::InvalidArgument(_)));
        assert!(mock.calls().is_empty());
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
    fn pane_widths_size_the_layout() {
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
        // 70% of the 120-column window; the other pane gets the rest.
        assert!(mock.called("select_layout(%0,2a7e,120x40,0,0{84x40,0,0,0,35x40,85,0,1})"));
    }
}
