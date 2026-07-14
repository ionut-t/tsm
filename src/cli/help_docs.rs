use std::collections::HashMap;

use serde::Deserialize;

/// Curated documentation for tsm and tmux commands, embedded at compile time.
///
/// tsm command descriptions come from clap; here we only add usage examples.
/// tmux commands get both a human description and examples (tmux's own CLI only
/// exposes argument syntax, which is unreadable as a list column).
const DOCS_TOML: &str = include_str!("help_docs.toml");

#[derive(Deserialize)]
struct RawDocs {
    #[serde(default)]
    tsm: Vec<CommandDoc>,
    #[serde(default)]
    tmux: Vec<CommandDoc>,
}

#[derive(Deserialize, Clone)]
pub struct CommandDoc {
    pub name: String,
    /// Short one-line description (tmux only; tsm uses clap's `about`).
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub examples: Vec<Example>,
}

#[derive(Deserialize, Clone)]
pub struct Example {
    /// What the example does (rendered as a comment line).
    pub info: String,
    /// The command to run.
    pub cmd: String,
}

/// Parsed docs, indexed by command name for O(1) lookup.
pub struct Docs {
    pub tsm: HashMap<String, CommandDoc>,
    pub tmux: HashMap<String, CommandDoc>,
}

impl Docs {
    /// Parse the embedded docs. The data is compiled in, so a parse failure is a
    /// build-time authoring bug — surfaced loudly rather than silently ignored.
    pub fn load() -> Self {
        let raw: RawDocs =
            toml::from_str(DOCS_TOML).expect("embedded help_docs.toml must be valid TOML");
        Self {
            tsm: raw.tsm.into_iter().map(|d| (d.name.clone(), d)).collect(),
            tmux: raw.tmux.into_iter().map(|d| (d.name.clone(), d)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_docs_parse() {
        let docs = Docs::load();
        assert!(!docs.tmux.is_empty(), "tmux docs should not be empty");
        assert!(!docs.tsm.is_empty(), "tsm docs should not be empty");
    }
}
