use super::Window;
use crate::error::{Result, TsmError};
use std::process::Command;

pub struct TmuxClient;

impl TmuxClient {
    pub fn new() -> Self {
        TmuxClient
    }

    fn tmux_cmd(&self) -> Command {
        Command::new("tmux")
    }

    pub fn is_inside_tmux(&self) -> bool {
        std::env::var("TMUX").is_ok()
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

    pub fn list_windows(&self) -> Vec<Window> {
        self.tmux_cmd()
            .arg("list-windows")
            .arg("-a")
            .arg("-F")
            .arg("#{session_name}\t#{window_index}\t#{window_name}\t#{pane_id}")
            .output()
            .map(|output| {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    stdout
                        .lines()
                        .filter_map(|line| {
                            let parts: Vec<&str> = line.split('\t').collect();
                            if parts.len() >= 4 {
                                Some(Window {
                                    session_name: parts[0].to_string(),
                                    index: parts[1].parse().ok()?,
                                    name: parts[2].to_string(),
                                    pane_id: parts[3].to_string(),
                                })
                            } else {
                                None
                            }
                        })
                        .collect()
                } else {
                    vec![]
                }
            })
            .unwrap_or_else(|_| vec![])
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
