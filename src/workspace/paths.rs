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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::with_env;

    #[test]
    fn tsm_config_dir_takes_precedence() {
        with_env(
            &[
                ("TSM_CONFIG_DIR", Some("/custom/cfg")),
                ("XDG_CONFIG_HOME", Some("/xdg")),
            ],
            || {
                assert_eq!(workspaces_dir(), PathBuf::from("/custom/cfg/workspaces"));
            },
        );
    }

    #[test]
    fn xdg_config_home_used_when_tsm_dir_absent() {
        with_env(
            &[("TSM_CONFIG_DIR", None), ("XDG_CONFIG_HOME", Some("/xdg"))],
            || {
                assert_eq!(workspaces_dir(), PathBuf::from("/xdg/tsm/workspaces"));
            },
        );
    }

    #[test]
    fn falls_back_to_home_config() {
        with_env(
            &[
                ("TSM_CONFIG_DIR", None),
                ("XDG_CONFIG_HOME", None),
                ("HOME", Some("/home/tester")),
            ],
            || {
                assert_eq!(
                    workspaces_dir(),
                    PathBuf::from("/home/tester/.config/tsm/workspaces")
                );
            },
        );
    }
}
