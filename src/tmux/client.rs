use super::Window;
use crate::error::{Result, TsmError};
use std::collections::HashMap;
use std::process::Command;

pub struct TmuxClient;

/// Abstraction over the tmux operations the CLI relies on.
///
/// Commands take `&dyn Tmux` rather than the concrete [`TmuxClient`], so their
/// orchestration logic can be exercised in tests against an in-memory double
/// without spawning a real tmux server.
pub trait Tmux {
    fn is_inside_tmux(&self) -> bool;

    fn new_window(
        &self,
        session: &str,
        name: Option<&str>,
        path: Option<&std::path::Path>,
        env: &HashMap<String, String>,
    ) -> Result<usize>;

    fn rename_window(&self, session: &str, new_name: &str) -> Result<()>;

    fn split_horizontal(
        &self,
        target_pane: &str,
        path: Option<&std::path::Path>,
        percentage: Option<u32>,
        env: &HashMap<String, String>,
    ) -> Result<String>;

    fn split_vertical(
        &self,
        target_pane: &str,
        path: Option<&std::path::Path>,
        percentage: Option<u32>,
        env: &HashMap<String, String>,
    ) -> Result<String>;

    fn resize_pane_height(&self, pane_id: &str, percentage: u32) -> Result<()>;

    fn resize_pane_width(&self, pane_id: &str, percentage: u32) -> Result<()>;

    fn send_keys(&self, pane_id: &str, command: &str) -> Result<()>;

    fn select_pane(&self, pane_id: &str) -> Result<()>;

    fn list_panes(&self, session: &str, window_index: usize) -> Result<Vec<String>>;

    fn get_current_window_index(&self, session: &str) -> Result<usize>;

    fn current_session(&self) -> Result<String>;

    /// List session names, most-recently-attached first.
    ///
    /// Spawn failure (e.g. tmux not installed) propagates as an error; a
    /// non-zero tmux exit (no server running) is not an error — it yields an
    /// empty list, so bootstrap flows like `tsm new` still work from scratch.
    fn list_sessions(&self) -> Result<Vec<String>>;

    fn list_windows(&self) -> Result<Vec<Window>>;

    fn create_session_detached(
        &self,
        name: &str,
        path: &std::path::Path,
        env: &HashMap<String, String>,
    ) -> Result<()>;

    fn respawn_pane(
        &self,
        pane_id: &str,
        path: &std::path::Path,
        env: &HashMap<String, String>,
    ) -> Result<()>;

    fn new_session(&self, name: String, path: String) -> Result<()>;

    fn select_window(&self, session: &str, window_index: usize) -> Result<()>;

    fn kill_session(&self, session: &str) -> Result<()>;

    fn kill_all_sessions(&self) -> Result<()>;

    fn rename_session(&self, current_name: Option<&str>, new_name: &str) -> Result<()>;

    fn attach_session(&self, session: &str) -> Result<()>;

    fn switch_session(&self, name: &str) -> Result<()>;

    fn switch_to_window(&self, session: &str, window_index: u32) -> Result<()>;

    fn attach_to_window(&self, session: &str, window_index: u32) -> Result<()>;

    fn get_current_window(&self) -> Result<(String, u32)>;

    fn move_window(&self, from_session: &str, from_index: u32, to_session: &str) -> Result<()>;

    fn get_pane_id(&self, session: &str, window_index: u32) -> Result<String>;

    fn find_window_by_pane_id(&self, pane_id: &str) -> Result<(String, u32)>;

    fn swap_windows(&self, source_index: u32, target_index: u32) -> Result<()>;

    fn is_last_window_in_session(&self, session: &str) -> Result<bool>;

    fn display_message(&self, message: &str) -> Result<()>;
}

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

    fn list_sorted_sessions(&self) -> Result<Vec<(String, u64)>> {
        let output = self
            .tmux_cmd()
            .arg("list-sessions")
            .arg("-F")
            .arg("#{session_name}:#{session_last_attached}")
            .output()?; // spawn failure (e.g. tmux not installed) propagates.

        // A non-zero exit is the normal "no server running" case (`tmux
        // list-sessions` exits 1 with no server) — that means zero sessions,
        // NOT an error. Treating it as an error would break `tsm new`, which
        // lists sessions before starting the first one from scratch.
        let mut sessions = if output.status.success() {
            parse_session_lines(&String::from_utf8_lossy(&output.stdout))
        } else {
            Vec::new()
        };

        sessions.sort_by_key(|s| std::cmp::Reverse(s.1));
        Ok(sessions)
    }
}

/// Parse `tmux list-sessions -F '#{session_name}:#{session_last_attached}'`
/// output into `(name, last_attached)` pairs, skipping any malformed line.
fn parse_session_lines(stdout: &str) -> Vec<(String, u64)> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(2, ':');
            match (parts.next(), parts.next()) {
                (Some(name), Some(timestamp)) => timestamp
                    .trim()
                    .parse::<u64>()
                    .ok()
                    .map(|time| (name.to_string(), time)),
                _ => None,
            }
        })
        .collect()
}

impl Tmux for TmuxClient {
    fn is_inside_tmux(&self) -> bool {
        std::env::var("TMUX").is_ok()
    }

    fn new_window(
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

    fn rename_window(&self, session: &str, new_name: &str) -> Result<()> {
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
    fn split_horizontal(
        &self,
        target_pane: &str,
        path: Option<&std::path::Path>,
        percentage: Option<u32>,
        env: &HashMap<String, String>,
    ) -> Result<String> {
        self.split_pane_internal(target_pane, "-h", path, percentage, env)
    }

    /// Split a pane vertically (creates panes stacked top/bottom)
    fn split_vertical(
        &self,
        target_pane: &str,
        path: Option<&std::path::Path>,
        percentage: Option<u32>,
        env: &HashMap<String, String>,
    ) -> Result<String> {
        self.split_pane_internal(target_pane, "-v", path, percentage, env)
    }

    /// Resize a pane to a percentage of the window height
    fn resize_pane_height(&self, pane_id: &str, percentage: u32) -> Result<()> {
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
    fn resize_pane_width(&self, pane_id: &str, percentage: u32) -> Result<()> {
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

    fn send_keys(&self, pane_id: &str, command: &str) -> Result<()> {
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

    fn select_pane(&self, pane_id: &str) -> Result<()> {
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

    fn list_panes(&self, session: &str, window_index: usize) -> Result<Vec<String>> {
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

    fn get_current_window_index(&self, session: &str) -> Result<usize> {
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

    fn current_session(&self) -> Result<String> {
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

    fn list_sessions(&self) -> Result<Vec<String>> {
        Ok(self
            .list_sorted_sessions()?
            .into_iter()
            .map(|(name, _)| name)
            .collect())
    }

    fn list_windows(&self) -> Result<Vec<Window>> {
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

    fn create_session_detached(
        &self,
        name: &str,
        path: &std::path::Path,
        env: &HashMap<String, String>,
    ) -> Result<()> {
        let mut cmd = self.tmux_cmd();
        // Detached sessions default to 80x24 until a client attaches. Size the
        // session from the launching terminal (`-x -`/`-y -`, tmux >= 3.2) so
        // splits, resizes, and commands run against the real geometry. tmux
        // reads the client size from its tty, but `output()` nulls stdin, so
        // pass ours through — without it `-x -` silently falls back to 80x24.
        cmd.arg("new-session")
            .arg("-d")
            .arg("-x")
            .arg("-")
            .arg("-y")
            .arg("-")
            .arg("-s")
            .arg(name)
            .arg("-c")
            .arg(path)
            .stdin(std::process::Stdio::inherit());

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
    fn respawn_pane(
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

    fn new_session(&self, name: String, path: String) -> Result<()> {
        let output = self
            .tmux_cmd()
            .arg("new-session")
            .arg("-d")
            .arg("-x")
            .arg("-")
            .arg("-y")
            .arg("-")
            .arg("-s")
            .arg(&name)
            .arg("-c")
            .arg(path)
            .stdin(std::process::Stdio::inherit())
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

    fn select_window(&self, session: &str, window_index: usize) -> Result<()> {
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

    fn kill_session(&self, session: &str) -> Result<()> {
        if self.is_inside_tmux() {
            let current = self.current_session().ok();

            if current.as_deref() == Some(session) {
                let sessions = self.list_sorted_sessions()?;

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

    fn kill_all_sessions(&self) -> Result<()> {
        self.tmux_cmd().arg("kill-server").output()?;
        Ok(())
    }

    fn rename_session(&self, current_name: Option<&str>, new_name: &str) -> Result<()> {
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

    fn attach_session(&self, session: &str) -> Result<()> {
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

    fn switch_session(&self, name: &str) -> Result<()> {
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

    fn switch_to_window(&self, session: &str, window_index: u32) -> Result<()> {
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

    fn attach_to_window(&self, session: &str, window_index: u32) -> Result<()> {
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

    fn get_current_window(&self) -> Result<(String, u32)> {
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

    fn move_window(&self, from_session: &str, from_index: u32, to_session: &str) -> Result<()> {
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

    fn get_pane_id(&self, session: &str, window_index: u32) -> Result<String> {
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

    fn find_window_by_pane_id(&self, pane_id: &str) -> Result<(String, u32)> {
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

    fn swap_windows(&self, source_index: u32, target_index: u32) -> Result<()> {
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

    fn is_last_window_in_session(&self, session: &str) -> Result<bool> {
        let windows = self.list_windows()?;
        let count = windows.iter().filter(|w| w.session_name == session).count();
        Ok(count == 1)
    }

    fn display_message(&self, message: &str) -> Result<()> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::with_env;

    fn args_of(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn add_env_args_emits_sorted_dash_e_pairs() {
        let mut cmd = Command::new("tmux");
        let env = HashMap::from([
            ("ZED".to_string(), "1".to_string()),
            ("ALPHA".to_string(), "2".to_string()),
            ("MID".to_string(), "3".to_string()),
        ]);
        TmuxClient::add_env_args(&mut cmd, &env);

        // Sorted by key for deterministic command construction.
        assert_eq!(
            args_of(&cmd),
            vec!["-e", "ALPHA=2", "-e", "MID=3", "-e", "ZED=1"]
        );
    }

    #[test]
    fn add_env_args_with_empty_map_adds_nothing() {
        let mut cmd = Command::new("tmux");
        TmuxClient::add_env_args(&mut cmd, &HashMap::new());
        assert!(args_of(&cmd).is_empty());
    }

    #[test]
    fn add_env_args_preserves_values_with_equals_and_spaces() {
        let mut cmd = Command::new("tmux");
        let env = HashMap::from([("K".to_string(), "a=b c".to_string())]);
        TmuxClient::add_env_args(&mut cmd, &env);
        assert_eq!(args_of(&cmd), vec!["-e", "K=a=b c"]);
    }

    #[test]
    fn parse_session_lines_extracts_name_and_timestamp() {
        let out = "work:1700000000\nplay:1699999999\n";
        assert_eq!(
            parse_session_lines(out),
            vec![
                ("work".to_string(), 1_700_000_000),
                ("play".to_string(), 1_699_999_999),
            ]
        );
    }

    #[test]
    fn parse_session_lines_keeps_only_the_first_colon_as_separator() {
        // Session names may contain colons; splitn(2) keeps the rest as the
        // timestamp field, which then fails to parse and drops the line.
        assert_eq!(
            parse_session_lines("a:b:1700000000\ngood:5\n"),
            vec![("good".to_string(), 5)]
        );
    }

    #[test]
    fn parse_session_lines_skips_malformed_and_empty_lines() {
        let out = "\nnocolon\nname:notanumber\n\nok:42\n";
        assert_eq!(parse_session_lines(out), vec![("ok".to_string(), 42)]);
    }

    #[test]
    fn is_inside_tmux_reflects_tmux_env_var() {
        let client = TmuxClient::new();
        with_env(&[("TMUX", Some("/tmp/tmux-1000/default,1234,0"))], || {
            assert!(client.is_inside_tmux());
        });
        with_env(&[("TMUX", None)], || {
            assert!(!client.is_inside_tmux());
        });
    }
}
