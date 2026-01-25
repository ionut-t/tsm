use crate::error::Result;
use serde::Deserialize;

use super::paths::workspaces_dir;

#[derive(Debug, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub root: Option<String>,
    #[serde(default)]
    pub window: Vec<Window>,
}

#[derive(Debug, Deserialize)]
pub struct Window {
    pub name: Option<String>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default)]
    pub row: Vec<Row>,
}

#[derive(Debug, Deserialize)]
pub struct Row {
    /// Height as percentage (1-100). If not specified, rows split evenly.
    pub height: Option<u32>,
    #[serde(default)]
    pub pane: Vec<Pane>,
}

#[derive(Debug, Deserialize)]
pub struct Pane {
    pub command: Option<String>,
    /// Width as percentage (1-100). If not specified, panes split evenly.
    pub width: Option<u32>,
    #[serde(default)]
    pub focus: bool,
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
