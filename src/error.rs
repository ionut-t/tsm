use std::io;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TsmError {
    #[error("Not inside a tmux session")]
    NotInTmux,

    #[error("failed to execute tmux command: {0}")]
    TmuxCommand(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("failed to save history to {path}: {source}")]
    HistoryPersist { path: PathBuf, source: io::Error },

    #[error("failed to execute fzf command: {0}")]
    Fzf(String),

    #[error("zoxide is not installed or failed to execute")]
    ZoxideQueryFailed,

    #[error("Home directory not found")]
    HomeDirectoryNotFound,

    #[error("{0}")]
    InvalidArgument(String),

    #[error("TOML deserialization error: {0}")]
    TomlDeserialization(#[from] toml::de::Error),

    #[error("Workspace '{0}' not found")]
    WorkspaceNotFound(String),

    #[error("No workspaces found")]
    NoWorkspacesFound,

    #[error("Workspace '{0}' already exists")]
    WorkspaceAlreadyExists(String),
}

pub type Result<T> = std::result::Result<T, TsmError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_render_arguments() {
        assert_eq!(TsmError::NotInTmux.to_string(), "Not inside a tmux session");
        assert_eq!(
            TsmError::TmuxCommand("boom".into()).to_string(),
            "failed to execute tmux command: boom"
        );
        assert_eq!(
            TsmError::Fzf("nope".into()).to_string(),
            "failed to execute fzf command: nope"
        );
        assert_eq!(
            TsmError::ZoxideQueryFailed.to_string(),
            "zoxide is not installed or failed to execute"
        );
        assert_eq!(
            TsmError::HomeDirectoryNotFound.to_string(),
            "Home directory not found"
        );
        // InvalidArgument is transparent — it renders only the inner message.
        assert_eq!(
            TsmError::InvalidArgument("bad flag".into()).to_string(),
            "bad flag"
        );
        assert_eq!(
            TsmError::WorkspaceNotFound("dev".into()).to_string(),
            "Workspace 'dev' not found"
        );
        assert_eq!(
            TsmError::NoWorkspacesFound.to_string(),
            "No workspaces found"
        );
        assert_eq!(
            TsmError::WorkspaceAlreadyExists("dev".into()).to_string(),
            "Workspace 'dev' already exists"
        );
    }

    #[test]
    fn history_persist_names_the_target_path() {
        let err = TsmError::HistoryPersist {
            path: std::path::PathBuf::from("/home/user/.config/tsm/history"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        };
        assert_eq!(
            err.to_string(),
            "failed to save history to /home/user/.config/tsm/history: denied"
        );
    }

    #[test]
    fn converts_from_io_error() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err: TsmError = io.into();
        assert!(matches!(err, TsmError::Io(_)));
        assert_eq!(err.to_string(), "IO error: missing");
    }

    #[test]
    fn converts_from_toml_error() {
        let toml_err =
            toml::from_str::<crate::workspace::config::Workspace>("name = ").unwrap_err();
        let err: TsmError = toml_err.into();
        assert!(matches!(err, TsmError::TomlDeserialization(_)));
        assert!(err.to_string().starts_with("TOML deserialization error:"));
    }
}
