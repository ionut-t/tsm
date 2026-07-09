use super::Window;
use crate::error::{Result, TsmError};
use std::collections::HashMap;
use std::process::Command;

pub struct TmuxClient;

impl TmuxClient {
    pub fn new() -> Self {
        TmuxClient
    }

    fn tmux_cmd(&self) -> Command {
        Command::new("tmux")
    }

    /// Append `-e KEY=VALUE` arguments for each variable so tmux spawns the new
    /// window/pane with that environment. Args go straight to exec, so no shell
    /// quoting is required. Sorted for deterministic command construction.
    fn add_env_args(cmd: &mut Command, env: &HashMap<String, String>) {
        let mut vars: Vec<(&String, &String)> = env.iter().collect();
        vars.sort();
        for (key, value) in vars {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }
    }

    pub fn is_inside_tmux(&self) -> bool {
        std::env::var("TMUX").is_ok()
    }

    pub fn new_window(
        &self,
        session: &str,
        name: Option<&str>,
        path: Option<&std::path::Path>,
        env: &HashMap<String, String>,
    ) -> Result<usize> {
        let mut cmd = self.tmux_cmd();
        cmd.arg("new-window")
            .arg("-t")
            .arg(session)
            .arg("-d")
            .arg("-P")
            .arg("-F")
            .arg("#{window_index}");

        if let Some(window_name) = name {
            cmd.arg("-n").arg(window_name);
        }

        if let Some(p) = path {
            cmd.arg("-c").arg(p);
        }

        Self::add_env_args(&mut cmd, env);

        let output = cmd.output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.trim().parse::<usize>().map_err(|_| {
                TsmError::TmuxCommand(format!(
                    "Failed to parse window index from: '{}'",
                    stdout.trim()
                ))
            })
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn rename_window(&self, session: &str, new_name: &str) -> Result<()> {
        let output = self
            .tmux_cmd()
            .arg("rename-window")
            .arg("-t")
            .arg(session)
            .arg(new_name)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    /// Split a pane horizontally (creates panes side by side)
    pub fn split_horizontal(
        &self,
        target_pane: &str,
        path: Option<&std::path::Path>,
        percentage: Option<u32>,
        env: &HashMap<String, String>,
    ) -> Result<String> {
        self.split_pane_internal(target_pane, "-h", path, percentage, env)
    }

    /// Split a pane vertically (creates panes stacked top/bottom)
    pub fn split_vertical(
        &self,
        target_pane: &str,
        path: Option<&std::path::Path>,
        percentage: Option<u32>,
        env: &HashMap<String, String>,
    ) -> Result<String> {
        self.split_pane_internal(target_pane, "-v", path, percentage, env)
    }

    fn split_pane_internal(
        &self,
        target_pane: &str,
        split_flag: &str,
        path: Option<&std::path::Path>,
        percentage: Option<u32>,
        env: &HashMap<String, String>,
    ) -> Result<String> {
        let mut cmd = self.tmux_cmd();
        cmd.arg("split-window")
            .arg(split_flag)
            .arg("-t")
            .arg(target_pane)
            .arg("-P")
            .arg("-F")
            .arg("#{pane_id}");

        if let Some(p) = path {
            cmd.arg("-c").arg(p);
        }

        if let Some(pct) = percentage {
            cmd.arg("-p").arg(pct.to_string());
        }

        Self::add_env_args(&mut cmd, env);

        let output = cmd.output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(first_line) = stdout.lines().next() {
                Ok(first_line.to_string())
            } else {
                Err(TsmError::TmuxCommand(
                    "No pane ID returned from split-window".to_string(),
                ))
            }
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    /// Resize a pane to a percentage of the window height
    pub fn resize_pane_height(&self, pane_id: &str, percentage: u32) -> Result<()> {
        let output = self
            .tmux_cmd()
            .arg("resize-pane")
            .arg("-t")
            .arg(pane_id)
            .arg("-y")
            .arg(format!("{}%", percentage))
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    /// Resize a pane to a percentage of the window width
    pub fn resize_pane_width(&self, pane_id: &str, percentage: u32) -> Result<()> {
        let output = self
            .tmux_cmd()
            .arg("resize-pane")
            .arg("-t")
            .arg(pane_id)
            .arg("-x")
            .arg(format!("{}%", percentage))
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn send_keys(&self, pane_id: &str, command: &str) -> Result<()> {
        let output = self
            .tmux_cmd()
            .arg("send-keys")
            .arg("-t")
            .arg(pane_id)
            .arg(command)
            .arg("C-m")
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn select_pane(&self, pane_id: &str) -> Result<()> {
        let output = self
            .tmux_cmd()
            .arg("select-pane")
            .arg("-t")
            .arg(pane_id)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn list_panes(&self, session: &str, window_index: usize) -> Result<Vec<String>> {
        let output = self
            .tmux_cmd()
            .arg("list-panes")
            .arg("-t")
            .arg(format!("{}:{}", session, window_index))
            .arg("-F")
            .arg("#{pane_id}")
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let panes: Vec<String> = stdout.lines().map(|line| line.to_string()).collect();
            Ok(panes)
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn get_current_window_index(&self, session: &str) -> Result<usize> {
        let output = self
            .tmux_cmd()
            .arg("display-message")
            .arg("-p")
            .arg("-t")
            .arg(session)
            .arg("#{window_index}")
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let index_str = stdout.trim();

            index_str.parse::<usize>().map_err(|_| {
                TsmError::TmuxCommand("Failed to parse current window index".to_string())
            })
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn current_session(&self) -> Result<String> {
        let output = self
            .tmux_cmd()
            .arg("display-message")
            .arg("-p")
            .arg("#S")
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn list_sessions(&self) -> Vec<String> {
        self.list_sorted_sessions()
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    pub fn list_windows(&self) -> Result<Vec<Window>> {
        let output = self
            .tmux_cmd()
            .arg("list-windows")
            .arg("-a")
            .arg("-F")
            .arg("#{session_name}\t#{window_index}\t#{window_name}\t#{pane_id}")
            .output()?;

        if !output.status.success() {
            return Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let windows: Result<Vec<Window>> = stdout
            .lines()
            .map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 4 {
                    let window_index = parts[1].parse::<u32>().map_err(|_| {
                        TsmError::TmuxCommand(format!("Failed to parse window index: {}", parts[1]))
                    })?;

                    Ok(Window {
                        session_name: parts[0].to_string(),
                        index: window_index,
                        name: parts[2].to_string(),
                        pane_id: parts[3].to_string(),
                    })
                } else {
                    Err(TsmError::TmuxCommand(format!(
                        "Invalid window line format: {}",
                        line
                    )))
                }
            })
            .collect();

        windows
    }

    pub fn create_session_detached(
        &self,
        name: &str,
        path: &std::path::Path,
        env: &HashMap<String, String>,
    ) -> Result<()> {
        let mut cmd = self.tmux_cmd();
        cmd.arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(name)
            .arg("-c")
            .arg(path);

        Self::add_env_args(&mut cmd, env);

        let output = cmd.output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    /// Restart a pane's shell with a new environment. Used to give a window's
    /// initial pane its window/pane-level env without leaking those overrides
    /// into the session environment (as `new-session -e` would). `-k` kills the
    /// existing idle shell; `-e` is process-scoped, like `new-window`.
    pub fn respawn_pane(
        &self,
        pane_id: &str,
        path: &std::path::Path,
        env: &HashMap<String, String>,
    ) -> Result<()> {
        let mut cmd = self.tmux_cmd();
        cmd.arg("respawn-pane")
            .arg("-k")
            .arg("-t")
            .arg(pane_id)
            .arg("-c")
            .arg(path);

        Self::add_env_args(&mut cmd, env);

        let output = cmd.output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn new_session(&self, name: String, path: String) -> Result<()> {
        let output = self
            .tmux_cmd()
            .arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(&name)
            .arg("-c")
            .arg(path)
            .output()?;

        if output.status.success() {
            if self.is_inside_tmux() {
                return self.switch_session(&name);
            }

            self.attach_session(&name)
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn select_window(&self, session: &str, window_index: usize) -> Result<()> {
        let output = self
            .tmux_cmd()
            .arg("select-window")
            .arg("-t")
            .arg(format!("{}:{}", session, window_index))
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn kill_session(&self, session: &str) -> Result<()> {
        if self.is_inside_tmux() {
            let current = self.current_session().ok();

            if current.as_deref() == Some(session) {
                let sessions = self.list_sorted_sessions();

                if let Some((prev_session, _)) = sessions.iter().find(|(name, _)| name != session) {
                    self.switch_session(prev_session)?;
                }
            }
        }

        let output = self
            .tmux_cmd()
            .arg("kill-session")
            .arg("-t")
            .arg(session)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }

    pub fn kill_all_sessions(&self) -> Result<()> {
        self.tmux_cmd().arg("kill-server").output()?;
        Ok(())
    }

    pub fn rename_session(&self, current_name: Option<&str>, new_name: &str) -> Result<()> {
        let current_name = if let Some(name) = current_name {
            name.to_string()
        } else {
            if !self.is_inside_tmux() {
                return Err(TsmError::NotInTmux);
            }

            self.current_session()?
        };

        let output = self
            .tmux_cmd()
            .arg("rename-session")
            .arg("-t")
            .arg(current_name)
            .arg(new_name)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn attach_session(&self, session: &str) -> Result<()> {
        let status = self
            .tmux_cmd()
            .arg("attach-session")
            .arg("-t")
            .arg(session)
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                "Failed to attach to session".to_string(),
            ))
        }
    }

    pub fn switch_session(&self, name: &str) -> Result<()> {
        let output = self
            .tmux_cmd()
            .arg("switch-client")
            .arg("-t")
            .arg(name)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn switch_to_window(&self, session: &str, window_index: u32) -> Result<()> {
        let output = self
            .tmux_cmd()
            .arg("switch-client")
            .arg("-t")
            .arg(format!("{}:{}", session, window_index))
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn attach_to_window(&self, session: &str, window_index: u32) -> Result<()> {
        let status = self
            .tmux_cmd()
            .arg("attach-session")
            .arg("-t")
            .arg(format!("{}:{}", session, window_index))
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                "Failed to attach to window".to_string(),
            ))
        }
    }

    pub fn get_current_window(&self) -> Result<(String, u32)> {
        let output = self
            .tmux_cmd()
            .arg("display-message")
            .arg("-p")
            .arg("#{session_name}:#{window_index}")
            .output()?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout);
            let mut parts = result.trim().splitn(2, ':');

            match (parts.next(), parts.next()) {
                (Some(session), Some(window)) => window
                    .parse::<u32>()
                    .map(|window_index| (session.to_string(), window_index))
                    .map_err(|_| {
                        TsmError::TmuxCommand("Failed to parse current window".to_string())
                    }),
                _ => Err(TsmError::TmuxCommand(
                    "Failed to parse current window".to_string(),
                )),
            }
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn move_window(&self, from_session: &str, from_index: u32, to_session: &str) -> Result<()> {
        let output = self
            .tmux_cmd()
            .arg("move-window")
            .arg("-s")
            .arg(format!("{}:{}", from_session, from_index))
            .arg("-t")
            .arg(format!("{}:", to_session))
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn get_pane_id(&self, session: &str, window_index: u32) -> Result<String> {
        let output = self
            .tmux_cmd()
            .arg("display-message")
            .arg("-p")
            .arg("-t")
            .arg(format!("{}:{}", session, window_index))
            .arg("#{pane_id}")
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(first_line) = stdout.lines().next() {
                Ok(first_line.to_string())
            } else {
                Err(TsmError::TmuxCommand(
                    "No panes found in the specified window".to_string(),
                ))
            }
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn find_window_by_pane_id(&self, pane_id: &str) -> Result<(String, u32)> {
        let windows = self.list_windows()?;

        for window in windows {
            if window.pane_id == pane_id {
                return Ok((window.session_name, window.index));
            }
        }

        Err(TsmError::TmuxCommand(
            "No window found with the specified pane ID".to_string(),
        ))
    }

    pub fn swap_windows(&self, source_index: u32, target_index: u32) -> Result<()> {
        let (session_name, _) = self.get_current_window()?;

        let output = self
            .tmux_cmd()
            .arg("swap-window")
            .arg("-s")
            .arg(format!("{}:{}", session_name, source_index))
            .arg("-t")
            .arg(format!("{}:{}", session_name, target_index))
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    pub fn is_last_window_in_session(&self, session: &str) -> Result<bool> {
        let windows = self.list_windows()?;
        let count = windows.iter().filter(|w| w.session_name == session).count();
        Ok(count == 1)
    }

    pub fn display_message(&self, message: &str) -> Result<()> {
        if !self.is_inside_tmux() {
            println!("{}", message);
            return Ok(());
        }

        let output = self
            .tmux_cmd()
            .arg("display-message")
            .arg(message)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    fn list_sorted_sessions(&self) -> Vec<(String, u64)> {
        let mut sessions = self
            .tmux_cmd()
            .arg("list-sessions")
            .arg("-F")
            .arg("#{session_name}:#{session_last_attached}")
            .output()
            .map(|output| {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    stdout
                        .lines()
                        .filter_map(|line| {
                            let mut parts = line.splitn(2, ':');
                            if let (Some(name), Some(timestamp)) = (parts.next(), parts.next()) {
                                if let Ok(time) = timestamp.trim().parse::<u64>() {
                                    Some((name.to_string(), time))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .collect()
                } else {
                    vec![]
                }
            })
            .unwrap_or_else(|_| vec![]);

        sessions.sort_by(|a, b| b.1.cmp(&a.1));
        sessions
    }
}
