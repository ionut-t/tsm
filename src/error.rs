use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TsmError {
    #[error("Not inside a tmux session")]
    NotInTmux,

    #[error("failed to execute tmux command: {0}")]
    TmuxCommand(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

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
