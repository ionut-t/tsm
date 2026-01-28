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
