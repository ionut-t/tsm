use std::process::Command;

use crate::error::{Result, TsmError};

pub trait Client {
    fn is_inside_tmux(&self) -> bool;
    fn list_sessions(&self) -> Vec<String>;
    fn new_session(&self, name: Option<&str>, path: Option<&str>) -> Result<()>;
    fn kill_session(&self, session: Option<&str>) -> Result<()>;
    fn rename_session(&self, current_name: Option<&str>, new_name: &str) -> Result<()>;
    fn attach_session(&self, session: &str) -> Result<()>;
    fn switch_session(&self, name: &str) -> Result<()>;
    fn current_session(&self) -> Result<String>;
    fn display_message(&self, message: &str);
}

pub struct TmuxClient;

impl TmuxClient {
    fn tmux_cmd(&self) -> Command {
        Command::new("tmux")
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

impl Client for TmuxClient {
    fn is_inside_tmux(&self) -> bool {
        std::env::var("TMUX").is_ok()
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

    fn list_sessions(&self) -> Vec<String> {
        self.list_sorted_sessions()
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    fn new_session(&self, name: Option<&str>, path: Option<&str>) -> Result<()> {
        let session_name = name.unwrap_or_default();

        let output = self
            .tmux_cmd()
            .arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(session_name)
            .arg("-c")
            .arg(path.unwrap_or("."))
            .output()?;

        if output.status.success() {
            if self.is_inside_tmux() {
                return self.switch_session(session_name);
            }

            self.attach_session(session_name)
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    fn kill_session(&self, session: Option<&str>) -> Result<()> {
        let target = session.unwrap_or("default");

        if self.is_inside_tmux() {
            let current = self.current_session().ok();

            if current.as_deref() == Some(target) {
                let sessions = self.list_sorted_sessions();

                if let Some((prev_session, _)) = sessions.iter().find(|(name, _)| name != target) {
                    self.switch_session(prev_session)?;
                }
            }
        }

        let output = self
            .tmux_cmd()
            .arg("kill-session")
            .arg("-t")
            .arg(target)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(TsmError::TmuxCommand(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
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

    fn display_message(&self, message: &str) {
        self.tmux_cmd()
            .arg("display-message")
            .arg(message)
            .output()
            .ok();
    }
}
