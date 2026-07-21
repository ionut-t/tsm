use std::collections::HashMap;

use crate::error::Result;
use serde::Deserialize;

use super::paths::workspaces_dir;

#[derive(Debug, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub root: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub window: Vec<Window>,
}

#[derive(Debug, Deserialize)]
pub struct Window {
    pub name: Option<String>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub row: Vec<Row>,
}

#[derive(Debug, Deserialize)]
pub struct Row {
    pub height: Option<u32>,
    #[serde(default)]
    pub pane: Vec<Pane>,
}

#[derive(Debug, Deserialize)]
pub struct Pane {
    pub command: Option<String>,
    pub width: Option<u32>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Workspace {
    pub fn load(name: &str) -> Result<Workspace> {
        let path = workspaces_dir().join(format!("{}.toml", name));
        let config_str = std::fs::read_to_string(path)?;
        let workspace: Workspace = toml::from_str(&config_str)?;
        Ok(workspace)
    }

    pub fn list() -> Result<Vec<String>> {
        let mut workspaces = Vec::new();
        let dir = workspaces_dir();

        if !dir.exists() {
            return Ok(workspaces);
        }

        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if let Some(ext) = path.extension()
                && ext == "toml"
                && let Some(stem) = path.file_stem()
                && let Some(stem_str) = stem.to_str()
            {
                workspaces.push(stem_str.to_string());
            }
        }
        Ok(workspaces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::with_env;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn deserializes_full_workspace() {
        let toml = r#"
            name = "dev"
            root = "~/code"

            [env]
            GLOBAL = "1"

            [[window]]
            name = "editor"
            focus = true

            [window.env]
            WIN = "2"

            [[window.row]]
            height = 60

            [[window.row.pane]]
            command = "nvim ."
            width = 70
            focus = true

            [window.row.pane.env]
            PANE = "3"
        "#;

        let ws: Workspace = toml::from_str(toml).unwrap();
        assert_eq!(ws.name, "dev");
        assert_eq!(ws.root.as_deref(), Some("~/code"));
        assert_eq!(ws.env.get("GLOBAL").map(String::as_str), Some("1"));

        assert_eq!(ws.window.len(), 1);
        let window = &ws.window[0];
        assert_eq!(window.name.as_deref(), Some("editor"));
        assert!(window.focus);
        assert_eq!(window.env.get("WIN").map(String::as_str), Some("2"));

        let row = &window.row[0];
        assert_eq!(row.height, Some(60));
        let pane = &row.pane[0];
        assert_eq!(pane.command.as_deref(), Some("nvim ."));
        assert_eq!(pane.width, Some(70));
        assert!(pane.focus);
        assert_eq!(pane.env.get("PANE").map(String::as_str), Some("3"));
    }

    #[test]
    fn applies_defaults_for_minimal_workspace() {
        let ws: Workspace = toml::from_str(r#"name = "minimal""#).unwrap();
        assert_eq!(ws.name, "minimal");
        assert!(ws.root.is_none());
        assert!(ws.env.is_empty());
        assert!(ws.window.is_empty());
    }

    #[test]
    fn window_and_pane_fields_default_when_omitted() {
        let ws: Workspace = toml::from_str(
            r#"
                name = "d"
                [[window]]
                [[window.row]]
                [[window.row.pane]]
            "#,
        )
        .unwrap();
        let window = &ws.window[0];
        assert!(window.name.is_none());
        assert!(!window.focus);
        let pane = &window.row[0].pane[0];
        assert!(pane.command.is_none());
        assert!(pane.width.is_none());
        assert!(!pane.focus);
    }

    #[test]
    fn missing_required_name_is_an_error() {
        assert!(toml::from_str::<Workspace>(r#"root = "~/x""#).is_err());
    }

    #[test]
    fn list_returns_empty_when_directory_absent() {
        let tmp = TempDir::new().unwrap();
        // Point TSM_CONFIG_DIR at an empty dir so `workspaces/` does not exist.
        with_env(
            &[
                ("TSM_CONFIG_DIR", tmp.path().to_str()),
                ("XDG_CONFIG_HOME", None),
            ],
            || {
                assert!(Workspace::list().unwrap().is_empty());
            },
        );
    }

    #[test]
    fn list_returns_toml_stems_ignoring_other_files() {
        let tmp = TempDir::new().unwrap();
        let ws_dir = tmp.path().join("workspaces");
        fs::create_dir_all(&ws_dir).unwrap();
        fs::write(ws_dir.join("alpha.toml"), r#"name = "alpha""#).unwrap();
        fs::write(ws_dir.join("beta.toml"), r#"name = "beta""#).unwrap();
        fs::write(ws_dir.join("notes.txt"), "ignore me").unwrap();

        with_env(
            &[
                ("TSM_CONFIG_DIR", tmp.path().to_str()),
                ("XDG_CONFIG_HOME", None),
            ],
            || {
                let mut names = Workspace::list().unwrap();
                names.sort();
                assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
            },
        );
    }

    #[test]
    fn load_reads_named_workspace() {
        let tmp = TempDir::new().unwrap();
        let ws_dir = tmp.path().join("workspaces");
        fs::create_dir_all(&ws_dir).unwrap();
        fs::write(ws_dir.join("dev.toml"), r#"name = "dev-session""#).unwrap();

        with_env(
            &[
                ("TSM_CONFIG_DIR", tmp.path().to_str()),
                ("XDG_CONFIG_HOME", None),
            ],
            || {
                let ws = Workspace::load("dev").unwrap();
                assert_eq!(ws.name, "dev-session");
            },
        );
    }

    #[test]
    fn load_missing_workspace_is_an_error() {
        let tmp = TempDir::new().unwrap();
        with_env(
            &[
                ("TSM_CONFIG_DIR", tmp.path().to_str()),
                ("XDG_CONFIG_HOME", None),
            ],
            || {
                assert!(Workspace::load("nope").is_err());
            },
        );
    }
}
