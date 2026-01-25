use std::path::PathBuf;

pub fn workspaces_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TSM_CONFIG_DIR") {
        return PathBuf::from(dir).join("workspaces");
    }

    if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg_config).join("tsm").join("workspaces");
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("tsm")
        .join("workspaces")
}
