use std::env;
use std::fs;
use std::path::PathBuf;

/// Get the history file path with the following priority:
/// 1. TSM_HISTORY_FILE environment variable
/// 2. XDG_STATE_HOME/tsm/history (or ~/.local/state/tsm/history)
pub fn history_file_path() -> PathBuf {
    // Environment variable override
    if let Ok(custom_path) = env::var("TSM_HISTORY_FILE") {
        let path = PathBuf::from(custom_path);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        return path;
    }

    // XDG Base Directory (state)
    let xdg_path = if let Ok(xdg_state_home) = env::var("XDG_STATE_HOME") {
        PathBuf::from(xdg_state_home).join("tsm").join("history")
    } else if let Ok(home) = env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("tsm")
            .join("history")
    } else {
        PathBuf::from(".tsm_history")
    };

    // Ensure directory exists
    if let Some(parent) = xdg_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    xdg_path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::with_env;
    use tempfile::TempDir;

    #[test]
    fn tsm_history_file_takes_precedence_and_creates_parent() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("nested/dir/history");
        with_env(&[("TSM_HISTORY_FILE", target.to_str())], || {
            assert_eq!(history_file_path(), target);
        });
        assert!(
            target.parent().unwrap().exists(),
            "parent directory should be created"
        );
    }

    #[test]
    fn xdg_state_home_used_when_override_absent() {
        let tmp = TempDir::new().unwrap();
        with_env(
            &[
                ("TSM_HISTORY_FILE", None),
                ("XDG_STATE_HOME", tmp.path().to_str()),
            ],
            || {
                assert_eq!(history_file_path(), tmp.path().join("tsm").join("history"));
            },
        );
        assert!(tmp.path().join("tsm").exists());
    }

    #[test]
    fn falls_back_to_home_local_state() {
        let tmp = TempDir::new().unwrap();
        with_env(
            &[
                ("TSM_HISTORY_FILE", None),
                ("XDG_STATE_HOME", None),
                ("HOME", tmp.path().to_str()),
            ],
            || {
                assert_eq!(
                    history_file_path(),
                    tmp.path()
                        .join(".local")
                        .join("state")
                        .join("tsm")
                        .join("history")
                );
            },
        );
    }
}
